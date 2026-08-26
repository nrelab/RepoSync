//! Reference RepoSync plugin: `rename-prefix`.
//!
//! Demonstrates the M14 WASM plugin ABI end-to-end. It prefixes every file
//! path in the snapshot with the `args.prefix` string (for example, to move a
//! package's extracted files under a subdirectory).
//!
//! Build it to WASM and point a `custom:` transform at the resulting `.wasm`:
//!
//! ```sh
//! rustup target add wasm32-unknown-unknown
//! cargo build --target wasm32-unknown-unknown --release
//! # then: custom: { name: rename-prefix, path: target/.../reposync_plugin_rename_prefix.wasm, args: { prefix: "pkg/" } }
//! ```
//!
//! ABI (see `reposync-transform/src/custom.rs`):
//! - `memory`  — exported linear memory.
//! - `alloc(n)` — returns a pointer to `n` free bytes (bump allocator).
//! - `transform(ptr, len) -> ptr` — reads `len` bytes at `ptr` (a JSON
//!   `PluginInput`), rewrites the snapshot, and returns a pointer to a
//!   `[u32 len][bytes]` region holding the JSON `PluginInput` to keep.

use std::collections::BTreeMap;

use serde_json::Value;

/// Bump allocator over WASM linear memory. One-shot (never frees), which is
/// fine because a plugin transforms a single snapshot per call.
#[no_mangle]
pub extern "C" fn alloc(size: i32) -> i32 {
    // Start above the Rust stack/data; 64 KiB is safely past it for small
    // plugins. Grows linearly; `memory_grow` keeps us in bounds.
    const BASE: i32 = 65_536;
    static mut NEXT: i32 = BASE;

    unsafe {
        let start = NEXT;
        let end = start + size;
        let page = 65_536;
        let current_pages = core::arch::wasm32::memory_size(0);
        let needed_pages = (end + page - 1) / page;
        if needed_pages > current_pages {
            core::arch::wasm32::memory_grow(0, (needed_pages - current_pages) as usize);
        }
        NEXT = end;
        start
    }
}

/// Plugin entry point. Returns a pointer to `[u32 len][json bytes]`.
#[no_mangle]
pub extern "C" fn transform(ptr: i32, len: i32) -> i32 {
    let input: Value = {
        let bytes = unsafe { std::slice::from_raw_parts(ptr as usize, len as usize) };
        serde_json::from_slice(bytes).expect("plugin input must be valid PluginInput JSON")
    };

    let prefix = input
        .get("args")
        .and_then(|a| a.get("prefix"))
        .and_then(Value::as_str)
        .unwrap_or("");

    let mut output = input.clone();
    if let Some(files) = output
        .get_mut("snapshot")
        .and_then(|s| s.get_mut("files"))
        .and_then(Value::as_object_mut)
    {
        let mut renamed: BTreeMap<String, Value> = BTreeMap::new();
        for (path, entry) in files.iter_mut() {
            let new_path = format!("{prefix}{path}");
            entry["path"] = Value::String(new_path.clone());
            renamed.insert(new_path, entry.clone());
        }
        *files = renamed;
    }

    let bytes = serde_json::to_vec(&output).expect("failed to serialize plugin output");
    let region = alloc(bytes.len() as i32 + 4);
    unsafe {
        std::ptr::copy_nonoverlapping(
            bytes.as_ptr(),
            (region + 4) as *mut u8,
            bytes.len(),
        );
        *(region as *mut u32) = bytes.len() as u32;
    }
    region
}
