//! Filesystem bridging for RepoSync.
//!
//! - [`ingest`] walks a directory and produces a [`RepositorySnapshot`].
//! - [`materialize`] writes a snapshot into a directory.
//!
//! Round-tripping `ingest` → `materialize` → `ingest` is lossless for regular,
//! executable, and symlink files (symlinks require a Unix platform). The
//! snapshot model never operates directly on a working tree for transforms;
//! this crate is the boundary.

mod ingest;
mod materialize;

pub use ingest::ingest;
pub use materialize::materialize;

use std::path::PathBuf;

/// Errors produced by filesystem operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A filesystem operation failed.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// An entry cannot be represented in a snapshot (or vice versa).
    #[error("unsupported path `{path}`: {reason}")]
    Unsupported { path: PathBuf, reason: String },

    /// The repository model rejected a path derived from the filesystem.
    #[error("repository path error: {0}")]
    Core(#[from] reposync_core::Error),
}
