// WASM linear-memory addresses are 32-bit; converting between `usize` and
// `i32` for guest pointers is inherent to the ABI and always in-range here.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]

use std::collections::BTreeMap;
use std::path::Path;

use reposync_core::RepositorySnapshot;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use wasmtime::{Engine, Linker, Module, Store};

use crate::{Error, TransformContext, TransformEvent, TransformResult, Transformation};

/// Input handed to a WASM plugin, and the envelope it returns.
///
/// The host serializes this to JSON and passes the bytes to the plugin's
/// exported `transform(ptr, len)` function. The plugin returns a pointer to a
/// `[u32 len][bytes]` region containing a `PluginInput` as JSON whose
/// `snapshot` field is the transformed result (the `args` field is ignored on
/// the way back).
#[derive(Serialize, Deserialize)]
struct PluginInput {
    snapshot: RepositorySnapshot,
    args: JsonValue,
}

/// A custom transform implemented as a WebAssembly module (M14).
///
/// Plugins receive the working snapshot plus their configured `args` and
/// return a new snapshot. The ABI is deliberately JSON-over-bytes so plugins
/// can be authored in any language that compiles to WASM (Rust, AssemblyScript,
/// TinyGo, ...).
#[derive(Debug, Clone)]
pub struct PluginTransform {
    engine: Engine,
    module: Module,
    name: String,
    args: JsonValue,
}

impl PluginTransform {
    /// Compiles a plugin from raw WASM bytes with the given name and args.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Plugin`] if the module fails to compile or does not
    /// export the required `memory`, `alloc`, and `transform` symbols.
    pub fn from_wasm_bytes(
        name: &str,
        bytes: &[u8],
        args: &BTreeMap<String, serde_yaml::Value>,
    ) -> Result<Self, Error> {
        let engine = Engine::default();
        let module = Module::new(&engine, bytes).map_err(|error| Error::Plugin {
            name: name.to_owned(),
            message: format!("failed to compile module: {error}"),
        })?;
        let input_args = serde_json::to_value(args).map_err(|error| Error::Plugin {
            name: name.to_owned(),
            message: format!("invalid args: {error}"),
        })?;
        let transform = Self {
            engine,
            module,
            name: name.to_owned(),
            args: input_args,
        };
        transform.check_exports()?;
        Ok(transform)
    }

    /// Loads a plugin from a `.wasm` file.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Plugin`] if the file cannot be read or the module fails
    /// to compile.
    pub fn from_file(
        name: &str,
        path: &Path,
        args: &BTreeMap<String, serde_yaml::Value>,
    ) -> Result<Self, Error> {
        let bytes = std::fs::read(path).map_err(|error| Error::Plugin {
            name: name.to_owned(),
            message: format!("failed to read `{}`: {error}", path.display()),
        })?;
        Self::from_wasm_bytes(name, &bytes, args)
    }

    /// Verifies the module exports the ABI symbols.
    fn check_exports(&self) -> Result<(), Error> {
        let mut store = Store::new(&self.engine, ());
        let linker = Linker::new(&self.engine);
        let instance = linker
            .instantiate(&mut store, &self.module)
            .map_err(|error| Error::Plugin {
                name: self.name.clone(),
                message: format!("failed to instantiate: {error}"),
            })?;
        for symbol in ["memory", "alloc", "transform"] {
            let exists = match symbol {
                "memory" => instance.get_memory(&mut store, "memory").is_some(),
                other => instance.get_func(&mut store, other).is_some(),
            };
            if !exists {
                return Err(Error::Plugin {
                    name: self.name.clone(),
                    message: format!("plugin does not export required symbol `{symbol}`"),
                });
            }
        }
        Ok(())
    }

    /// Runs the plugin over `snapshot` and returns the rewritten snapshot.
    fn run(&self, snapshot: &RepositorySnapshot) -> Result<RepositorySnapshot, Error> {
        let input = PluginInput {
            snapshot: snapshot.clone(),
            args: self.args.clone(),
        };
        let input_bytes = serde_json::to_vec(&input).map_err(|error| Error::Plugin {
            name: self.name.clone(),
            message: format!("failed to serialize input: {error}"),
        })?;

        let mut store = Store::new(&self.engine, ());
        let linker = Linker::new(&self.engine);
        let instance = linker
            .instantiate(&mut store, &self.module)
            .map_err(|error| Error::Plugin {
                name: self.name.clone(),
                message: format!("failed to instantiate: {error}"),
            })?;

        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| Error::Plugin {
                name: self.name.clone(),
                message: "plugin does not export `memory`".to_owned(),
            })?;

        let alloc = instance
            .get_typed_func::<i32, i32>(&mut store, "alloc")
            .map_err(|error| Error::Plugin {
                name: self.name.clone(),
                message: format!("missing `alloc`: {error}"),
            })?;
        let input_ptr = alloc
            .call(&mut store, input_bytes.len() as i32)
            .map_err(|error| Error::Plugin {
                name: self.name.clone(),
                message: format!("`alloc` failed: {error}"),
            })?;

        memory
            .write(&mut store, input_ptr as usize, &input_bytes)
            .map_err(|error| Error::Plugin {
                name: self.name.clone(),
                message: format!("failed to write input: {error}"),
            })?;

        let transform = instance
            .get_typed_func::<(i32, i32), i32>(&mut store, "transform")
            .map_err(|error| Error::Plugin {
                name: self.name.clone(),
                message: format!("missing `transform`: {error}"),
            })?;
        let out_ptr = transform
            .call(
                &mut store,
                (input_ptr, input_bytes.len() as i32),
            )
            .map_err(|error| Error::Plugin {
                name: self.name.clone(),
                message: format!("`transform` failed: {error}"),
            })? as usize;

        let mut len_buf = [0u8; 4];
        memory
            .read(&mut store, out_ptr, &mut len_buf)
            .map_err(|error| Error::Plugin {
                name: self.name.clone(),
                message: format!("failed to read output length: {error}"),
            })?;
        let out_len = u32::from_le_bytes(len_buf) as usize;

        let mut out_buf = vec![0u8; out_len];
        memory
            .read(&mut store, out_ptr + 4, &mut out_buf)
            .map_err(|error| Error::Plugin {
                name: self.name.clone(),
                message: format!("failed to read output: {error}"),
            })?;

        let envelope: PluginInput = serde_json::from_slice(&out_buf).map_err(|error| Error::Plugin {
            name: self.name.clone(),
            message: format!("plugin returned invalid snapshot JSON: {error}"),
        })?;
        Ok(envelope.snapshot)
    }
}

impl Transformation for PluginTransform {
    fn name(&self) -> &'static str {
        "custom"
    }

    fn apply(&self, ctx: &mut TransformContext) -> Result<TransformResult, Error> {
        let old = ctx.snapshot.clone();
        let new = self.run(&old)?;
        let changed = count_changes(&old, &new);
        ctx.snapshot = new;
        Ok(TransformResult {
            changed,
            warnings: Vec::new(),
            event: TransformEvent::Rewrote { files: changed },
        })
    }
}

/// Number of files added, removed, or modified between two snapshots.
fn count_changes(old: &RepositorySnapshot, new: &RepositorySnapshot) -> usize {
    let mut changed = 0;
    for (path, entry) in &old.files {
        match new.files.get(path) {
            None => changed += 1,
            Some(next) if next.content != entry.content => changed += 1,
            _ => {}
        }
    }
    for path in new.files.keys() {
        if !old.files.contains_key(path) {
            changed += 1;
        }
    }
    changed
}
