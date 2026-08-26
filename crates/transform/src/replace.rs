use std::collections::BTreeMap;

use reposync_core::Blob;

use crate::glob::compile_all;
use crate::{Error, TransformContext, TransformEvent, TransformResult, Transformation};

/// Replace literal substrings within files matching a glob pattern.
///
/// This is the "raw string" upgrade path from the M13 plan: every occurrence
/// of each `old` key is replaced with its `new` value in each file whose path
/// matches the `file` glob. Binary (non-UTF-8) files are skipped with a warning.
#[derive(Debug, Clone)]
pub struct Replace {
    file: String,
    replacements: BTreeMap<String, String>,
}

impl Replace {
    /// Creates a substring replace over files matching `file` (a glob).
    #[must_use]
    pub fn new(file: impl Into<String>, replacements: BTreeMap<String, String>) -> Self {
        Self {
            file: file.into(),
            replacements,
        }
    }
}

impl Transformation for Replace {
    fn name(&self) -> &'static str {
        "replace"
    }

    fn apply(&self, ctx: &mut TransformContext) -> Result<TransformResult, Error> {
        let globs = compile_all(std::slice::from_ref(&self.file), self.name())?;
        let mut changed = 0usize;
        let mut warnings = Vec::new();

        for entry in ctx.snapshot.files.values_mut() {
            if !globs.iter().any(|glob| glob.matches(&entry.path)) {
                continue;
            }
            let Ok(text) = std::str::from_utf8(entry.content.content()) else {
                warnings.push(format!("`{}` is not valid UTF-8; skipped", entry.path));
                continue;
            };
            let mut new = text.to_owned();
            for (old, replacement) in &self.replacements {
                new = new.replace(old, replacement);
            }
            if new != text {
                entry.content = Blob::from_bytes(new.into_bytes());
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

#[cfg(test)]
mod tests {
    use super::*;
    use reposync_core::{Blob, FileEntry, RepoPath, RepositorySnapshot};

    fn snapshot() -> RepositorySnapshot {
        let mut s = RepositorySnapshot::new();
        s.insert(FileEntry::regular(
            RepoPath::new("src/index.ts").unwrap(),
            Blob::from_bytes(b"import x from '@internal/sdk';"),
        ));
        s.insert(FileEntry::regular(
            RepoPath::new("README.md").unwrap(),
            Blob::from_bytes(b"use @internal/sdk here"),
        ));
        s
    }

    #[test]
    fn replaces_across_matching_files() {
        let mut ctx = TransformContext::new(snapshot());
        let mut map = BTreeMap::new();
        map.insert("@internal/sdk".to_owned(), "@public/sdk".to_owned());
        let result = Replace::new("src/**", map).apply(&mut ctx).unwrap();
        assert_eq!(result.changed, 1);

        let rewritten = ctx.snapshot.get(&RepoPath::new("src/index.ts").unwrap()).unwrap();
        assert_eq!(
            std::str::from_utf8(rewritten.content.content()).unwrap(),
            "import x from '@public/sdk';"
        );

        // README.md did not match the glob, so it is untouched.
        let readme = ctx.snapshot.get(&RepoPath::new("README.md").unwrap()).unwrap();
        assert_eq!(
            std::str::from_utf8(readme.content.content()).unwrap(),
            "use @internal/sdk here"
        );
    }
}
