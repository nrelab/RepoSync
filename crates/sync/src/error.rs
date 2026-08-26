use thiserror::Error;

/// Errors produced by the sync engine.
#[derive(Debug, Error)]
pub enum Error {
    /// A git operation failed.
    #[error("git error: {0}")]
    Git(#[from] reposync_git::Error),

    /// A transform failed.
    #[error("transform error: {0}")]
    Transform(#[from] reposync_transform::Error),

    /// The repository model rejected a value.
    #[error("core model error: {0}")]
    Core(#[from] reposync_core::Error),
}
