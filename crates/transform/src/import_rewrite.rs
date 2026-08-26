use std::collections::BTreeMap;

use reposync_core::Blob;

use crate::{Error, TransformContext, TransformEvent, TransformResult, Transformation};

/// Rewrite module specifiers in source files (the M13 "AST-aware" path).
///
/// For each rename `old -> new`, any quoted specifier equal to `old` (or a
/// subpath `old/sub`) is rewritten to `new` (resp. `new/sub`). This covers the
/// common forms:
///
/// - `import x from '@internal/sdk'`
/// - `const x = require('@internal/sdk')`
/// - `` import('@internal/sdk') ``
/// - `export * from '@internal/sdk'`
///
/// Matching is restricted to source file extensions to avoid mangling unrelated
/// string literals. This is a regex/AST-lite implementation; full tree-sitter
/// parsing is a future enhancement.
#[derive(Debug, Clone)]
pub struct ImportRewrite {
    renames: Vec<(String, String)>,
}

impl ImportRewrite {
    /// Creates an import specifier rewriter from an `old -> new` map.
    #[must_use]
    pub fn new(renames: BTreeMap<String, String>) -> Self {
        Self {
            renames: renames.into_iter().collect(),
        }
    }
}

const SOURCE_EXTENSIONS: &[&str] = &[
    "ts", "tsx", "js", "jsx", "mjs", "cjs", "cts", "mts",
];

impl Transformation for ImportRewrite {
    fn name(&self) -> &'static str {
        "import_rewrite"
    }

    fn apply(&self, ctx: &mut TransformContext) -> Result<TransformResult, Error> {
        let mut changed = 0usize;
        for entry in ctx.snapshot.files.values_mut() {
            if !is_source_file(entry.path.as_str()) {
                continue;
            }
            let Ok(text) = std::str::from_utf8(entry.content.content()) else {
                continue;
            };
            let mut new = text.to_owned();
            for (old, new_target) in &self.renames {
                for quote in ['\'', '"'] {
                    // Exact specifier: `from '@old'`, `require('@old')`, ...
                    let exact = format!("{quote}{old}{quote}");
                    let exact_replacement = format!("{quote}{new_target}{quote}");
                    new = new.replace(&exact, &exact_replacement);

                    // Subpath specifier: `@old/sub` -> `@new/sub`.
                    let sub = format!("{quote}{old}/");
                    let sub_replacement = format!("{quote}{new_target}/");
                    new = new.replace(&sub, &sub_replacement);
                }
            }
            if new != text {
                entry.content = Blob::from_bytes(new.into_bytes());
                changed += 1;
            }
        }

        Ok(TransformResult {
            changed,
            warnings: Vec::new(),
            event: TransformEvent::Rewrote { files: changed },
        })
    }
}

/// Whether `path` ends with a recognized source-code extension.
fn is_source_file(path: &str) -> bool {
    let ext = path.rsplit('.').next().unwrap_or("");
    SOURCE_EXTENSIONS.contains(&ext)
}

#[cfg(test)]
mod tests {
    use super::*;
    use reposync_core::{Blob, FileEntry, RepoPath, RepositorySnapshot};

    fn snapshot() -> RepositorySnapshot {
        let mut s = RepositorySnapshot::new();
        s.insert(FileEntry::regular(
            RepoPath::new("src/index.ts").unwrap(),
            Blob::from_bytes(
                b"import x from '@internal/sdk';\n\
                  const y = require('@internal/sdk/sub');\n\
                  export * from '@internal/cli';\n\
                  const z = import('@internal/sdk');\n",
            ),
        ));
        s.insert(FileEntry::regular(
            RepoPath::new("package.json").unwrap(),
            Blob::from_bytes(b"{ \"note\": \"import '@internal/sdk'\" }"),
        ));
        s
    }

    #[test]
    fn rewrites_import_and_require_specifiers() {
        let mut ctx = TransformContext::new(snapshot());
        let mut renames = BTreeMap::new();
        renames.insert("@internal/sdk".to_owned(), "@public/sdk".to_owned());
        renames.insert("@internal/cli".to_owned(), "@public/cli".to_owned());

        let result = ImportRewrite::new(renames).apply(&mut ctx).unwrap();
        assert_eq!(result.changed, 1);

        let src = ctx
            .snapshot
            .get(&RepoPath::new("src/index.ts").unwrap())
            .unwrap();
        let text = std::str::from_utf8(src.content.content()).unwrap();
        assert!(text.contains("import x from '@public/sdk';"));
        assert!(text.contains("require('@public/sdk/sub')"));
        assert!(text.contains("export * from '@public/cli';"));
        assert!(text.contains("import('@public/sdk')"));

        // package.json is not a source file: not touched.
        let pkg = ctx
            .snapshot
            .get(&RepoPath::new("package.json").unwrap())
            .unwrap();
        assert!(std::str::from_utf8(pkg.content.content())
            .unwrap()
            .contains("@internal/sdk"));
    }
}
