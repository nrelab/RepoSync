use std::collections::BTreeMap;

use reposync_core::Blob;
use serde_json::Value;

use crate::{Error, TransformContext, TransformEvent, TransformResult, Transformation};

/// Rewrite `package.json` manifests when extracting packages from a monorepo.
///
/// Two rewrites are supported:
/// 1. Rename the package `name` and any dependency keys via `package_rename`.
/// 2. Pin `workspace:*` dependency versions to a concrete `workspace_version`.
///
/// Only files named `package.json` are touched; malformed JSON is skipped with
/// a warning so a single bad manifest never aborts the whole pipeline.
#[derive(Debug, Clone)]
pub struct DependencyRewrite {
    package_rename: BTreeMap<String, String>,
    workspace_version: Option<String>,
}

impl DependencyRewrite {
    /// Creates a dependency rewriter.
    #[must_use]
    pub fn new(
        package_rename: BTreeMap<String, String>,
        workspace_version: Option<String>,
    ) -> Self {
        Self {
            package_rename,
            workspace_version,
        }
    }
}

const DEP_SECTIONS: &[&str] = &[
    "dependencies",
    "devDependencies",
    "peerDependencies",
    "optionalDependencies",
];

impl Transformation for DependencyRewrite {
    fn name(&self) -> &'static str {
        "dependency_rewrite"
    }

    fn apply(&self, ctx: &mut TransformContext) -> Result<TransformResult, Error> {
        let mut changed = 0usize;
        let mut warnings = Vec::new();

        for entry in ctx.snapshot.files.values_mut() {
            if basename(entry.path.as_str()) != "package.json" {
                continue;
            }
            let Ok(text) = std::str::from_utf8(entry.content.content()) else {
                warnings.push(format!("`{}` is not valid UTF-8; skipped", entry.path));
                continue;
            };
            let mut doc: Value = match serde_json::from_str(text) {
                Ok(value) => value,
                Err(error) => {
                    warnings.push(format!("`{}` is not valid JSON: {error}; skipped", entry.path));
                    continue;
                }
            };
            if rewrite_document(&mut doc, &self.package_rename, self.workspace_version.as_deref()) {
                let out = serde_json::to_string_pretty(&doc).map_err(|error| Error::Transform {
                    name: self.name().to_owned(),
                    message: format!("failed to serialize `{}`: {error}", entry.path),
                })?;
                entry.content = Blob::from_bytes(out.into_bytes());
                changed += 1;
            }
        }

        Ok(TransformResult {
            changed,
            warnings,
            event: TransformEvent::Rewrote { files: changed },
        })
    }
}

/// Returns `true` if the document was modified.
fn rewrite_document(
    doc: &mut Value,
    rename: &BTreeMap<String, String>,
    workspace_version: Option<&str>,
) -> bool {
    let Some(obj) = doc.as_object_mut() else {
        return false;
    };
    let mut modified = false;

    // Rename the package's own `name`.
    if let Some(Value::String(name)) = obj.get("name") {
        if let Some(target) = rename.get(name) {
            obj.insert("name".to_owned(), Value::String(target.clone()));
            modified = true;
        }
    }

    // Rewrite dependency sections.
    for section in DEP_SECTIONS {
        let Some(Value::Object(deps)) = obj.get_mut(*section) else {
            continue;
        };
        let keys: Vec<String> = deps.keys().cloned().collect();
        for key in keys {
            let mut new_key = key.clone();
            let mut new_value = deps.get(&key).cloned().unwrap();

            if let Some(target) = rename.get(&key) {
                new_key.clone_from(target);
                modified = true;
            }
            if let Value::String(version) = &new_value {
                if version.starts_with("workspace:") {
                    if let Some(pinned) = workspace_version {
                        new_value = Value::String(pinned.to_owned());
                        modified = true;
                    }
                }
            }

            if new_key != key || &new_value != deps.get(&key).unwrap() {
                deps.remove(&key);
                deps.insert(new_key, new_value);
                modified = true;
            }
        }
    }

    modified
}

/// The final path segment, or the whole path when there is no separator.
fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use reposync_core::{Blob, FileEntry, RepoPath, RepositorySnapshot};

    fn snapshot() -> RepositorySnapshot {
        let mut s = RepositorySnapshot::new();
        let manifest = r#"{
  "name": "@internal/sdk",
  "version": "0.0.0",
  "dependencies": {
    "@internal/cli": "workspace:*",
    "lodash": "^4.17.0"
  },
  "devDependencies": {
    "@internal/sdk": "workspace:^"
  }
}"#;
        s.insert(FileEntry::regular(
            RepoPath::new("packages/sdk/package.json").unwrap(),
            Blob::from_bytes(manifest.as_bytes()),
        ));
        s
    }

    #[test]
    fn rewrites_names_and_pins_workspace_versions() {
        let mut ctx = TransformContext::new(snapshot());
        let mut rename = BTreeMap::new();
        rename.insert("@internal/sdk".to_owned(), "@public/sdk".to_owned());
        rename.insert("@internal/cli".to_owned(), "@public/cli".to_owned());

        let result = DependencyRewrite::new(rename, Some("1.2.3".to_owned()))
            .apply(&mut ctx)
            .unwrap();
        assert_eq!(result.changed, 1);

        let manifest = ctx
            .snapshot
            .get(&RepoPath::new("packages/sdk/package.json").unwrap())
            .unwrap();
        let doc: Value =
            serde_json::from_slice(manifest.content.content()).expect("valid json output");
        assert_eq!(doc["name"], Value::String("@public/sdk".to_owned()));
        assert_eq!(
            doc["dependencies"]["@public/cli"],
            Value::String("1.2.3".to_owned())
        );
        assert_eq!(doc["dependencies"]["lodash"], Value::String("^4.17.0".to_owned()));
        assert_eq!(
            doc["devDependencies"]["@public/sdk"],
            Value::String("1.2.3".to_owned())
        );
    }
}
