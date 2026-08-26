//! Errors produced by the Git layer.
use thiserror::Error;

/// Errors produced by the Git layer.
#[derive(Debug, Error)]
pub enum Error {
    /// A libgit2 operation failed.
    #[error("git operation failed: {0}")]
    Git(#[from] git2::Error),

    /// A filesystem operation failed.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// The repository model rejected a value derived from git.
    #[error("core model error: {0}")]
    Core(#[from] reposync_core::Error),

    /// HEAD cannot be resolved to a branch name.
    #[error("HEAD is detached; cannot determine branch")]
    DetachedHead,

    /// A tree entry has a mode the model cannot represent.
    #[error("tree entry `{name}` has unsupported mode 0o{mode:o}")]
    UnsupportedEntry { name: String, mode: u32 },

    /// A snapshot contains a file and a directory at the same path.
    #[error("snapshot path `{path}` conflicts with a directory at the same location")]
    PathConflict { path: String },
}
