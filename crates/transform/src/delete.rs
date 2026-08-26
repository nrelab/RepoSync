use reposync_core::RepoPath;

use crate::glob::compile_all;
use crate::{Error, TransformContext, TransformEvent, TransformResult, Transformation};

/// Remove files matching any glob pattern.
///
/// Patterns like `**/secrets/**` are the intended use; see [`Glob`](crate::Glob).
#[derive(Debug, Clone)]
pub struct Delete {
    paths: Vec<String>,
}

impl Delete {
    /// Creates a delete over the given path patterns.
    #[must_use]
    pub fn new(paths: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            paths: paths.into_iter().map(Into::into).collect(),
        }
    }
}

impl Transformation for Delete {
    fn name(&self) -> &'static str {
        "delete"
    }

    fn apply(&self, ctx: &mut TransformContext) -> Result<TransformResult, Error> {
        let globs = compile_all(&self.paths, self.name())?;
        let removed: Vec<RepoPath> = ctx
            .snapshot
            .paths()
            .filter(|path| globs.iter().any(|glob| glob.matches(path)))
            .cloned()
            .collect();
        for path in &removed {
            ctx.snapshot.remove(path);
        }
        Ok(TransformResult {
            changed: removed.len(),
            warnings: Vec::new(),
            event: TransformEvent::Deleted { paths: removed },
        })
    }
}
