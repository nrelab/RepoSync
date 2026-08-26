use reposync_core::{RepoPath, RepositorySnapshot};

/// Mutable state threaded through a transform pipeline.
///
/// Each transform reads and mutates [`Self::snapshot`] in place and the
/// [`Runner`](crate::Runner) accumulates the shared run state.
#[derive(Debug, Clone)]
pub struct TransformContext {
    /// The working snapshot, mutated in place by each transform.
    pub snapshot: RepositorySnapshot,
    /// Warnings accumulated by earlier transforms in this run.
    pub warnings: Vec<String>,
    /// Structured events accumulated by earlier transforms in this run.
    pub events: Vec<TransformEvent>,
    /// Dry-run flag. Transforms never have side effects, so this only affects
    /// runner bookkeeping and reporting.
    pub dry_run: bool,
}

impl TransformContext {
    /// Creates a context for a pipeline run over `snapshot`.
    #[must_use]
    pub fn new(snapshot: RepositorySnapshot) -> Self {
        Self {
            snapshot,
            warnings: Vec::new(),
            events: Vec::new(),
            dry_run: false,
        }
    }

    /// Marks the run as a dry run.
    #[must_use]
    pub fn dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }
}

/// The outcome of a single transformation step.
#[derive(Debug, Clone)]
pub struct TransformResult {
    /// Number of files changed by this step (added, removed, or relocated).
    pub changed: usize,
    /// Human-readable warnings raised by this step.
    pub warnings: Vec<String>,
    /// Structured description of what this step did.
    pub event: TransformEvent,
}

/// Structured description of what a transform did to the snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransformEvent {
    /// A filter keep-set was applied.
    Filtered { kept: usize, removed: usize },
    /// Files were removed by path globs.
    Deleted { paths: Vec<RepoPath> },
    /// A subtree was copied to a destination, originals kept.
    Copied {
        from: String,
        to: String,
        paths: Vec<RepoPath>,
    },
    /// A subtree was moved to a destination.
    Moved {
        from: String,
        to: String,
        paths: Vec<RepoPath>,
    },
    /// A single file was renamed.
    Renamed { from: RepoPath, to: RepoPath },
    /// File contents were rewritten in place (replace / dependency / import).
    Rewrote { files: usize },
}
