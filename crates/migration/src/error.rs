//! Errors produced by the migration engine.

use thiserror::Error;

/// Errors produced while replaying history or synchronizing repositories.
#[derive(Debug, Error)]
pub enum Error {
    /// A git backend error.
    #[error("git error: {0}")]
    Git(#[from] reposync_git::Error),
    /// A state database error.
    #[error("state error: {0}")]
    State(#[from] reposync_state::Error),
    /// A transformation engine error.
    #[error("transform error: {0}")]
    Transform(#[from] reposync_transform::Error),
    /// A bidirectional sync found conflicting external changes.
    #[error("sync conflicts detected: {0:?}")]
    Conflict(crate::SyncReport),
}
