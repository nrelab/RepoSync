use crate::{Error, Move, TransformContext, Transformation, TransformResult};

/// Strip a leading path prefix from every file under it.
///
/// Concretely, `packages/sdk` turns `packages/sdk/src/lib.ts` into
/// `src/lib.ts`. This is implemented as a [`Move`] to the repository root.
#[derive(Debug, Clone)]
pub struct StripPrefix {
    prefix: String,
}

impl StripPrefix {
    /// Creates a prefix strip for `prefix`.
    #[must_use]
    pub fn new(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
        }
    }
}

impl Transformation for StripPrefix {
    fn name(&self) -> &'static str {
        "strip_prefix"
    }

    fn apply(&self, ctx: &mut TransformContext) -> Result<TransformResult, Error> {
        Move::new(&self.prefix, ".").apply(ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reposync_core::{Blob, FileEntry, RepoPath, RepositorySnapshot};

    fn snapshot() -> RepositorySnapshot {
        let mut s = RepositorySnapshot::new();
        s.insert(FileEntry::regular(
            RepoPath::new("packages/sdk/src/lib.ts").unwrap(),
            Blob::from_bytes(b"export const x = 1;"),
        ));
        s.insert(FileEntry::regular(
            RepoPath::new("other/file.ts").unwrap(),
            Blob::from_bytes(b"keep"),
        ));
        s
    }

    #[test]
    fn strips_leading_prefix() {
        let mut ctx = TransformContext::new(snapshot());
        let result = StripPrefix::new("packages/sdk").apply(&mut ctx).unwrap();
        assert_eq!(result.changed, 1);
        assert!(ctx.snapshot.contains(&RepoPath::new("src/lib.ts").unwrap()));
        assert!(!ctx
            .snapshot
            .contains(&RepoPath::new("packages/sdk/src/lib.ts").unwrap()));
        assert!(ctx
            .snapshot
            .contains(&RepoPath::new("other/file.ts").unwrap()));
    }
}
