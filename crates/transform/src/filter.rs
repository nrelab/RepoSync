use std::collections::BTreeMap;

use reposync_core::RepositorySnapshot;

use crate::glob::compile_all;
use crate::{Error, TransformContext, TransformEvent, TransformResult, Transformation};

/// Keep only files matching at least one glob pattern.
///
/// A file survives if any of the configured patterns matches its full path.
#[derive(Debug, Clone)]
pub struct Filter {
    paths: Vec<String>,
}

impl Filter {
    /// Creates a filter over the given keep-set patterns.
    #[must_use]
    pub fn new(paths: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            paths: paths.into_iter().map(Into::into).collect(),
        }
    }
}

impl Transformation for Filter {
    fn name(&self) -> &'static str {
        "filter"
    }

    fn apply(&self, ctx: &mut TransformContext) -> Result<TransformResult, Error> {
        let globs = compile_all(&self.paths, self.name())?;
        let mut kept: BTreeMap<_, _> = BTreeMap::new();
        for (path, entry) in &ctx.snapshot.files {
            if globs.iter().any(|glob| glob.matches(path)) {
                kept.insert(path.clone(), entry.clone());
            }
        }
        let removed = ctx.snapshot.len() - kept.len();
        ctx.snapshot = RepositorySnapshot::from_files(kept);
        Ok(TransformResult {
            changed: removed,
            warnings: Vec::new(),
            event: TransformEvent::Filtered {
                kept: ctx.snapshot.len(),
                removed,
            },
        })
    }
}
