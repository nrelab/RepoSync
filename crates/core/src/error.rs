use thiserror::Error;

/// Errors produced by the repository model.
#[derive(Debug, Error)]
pub enum Error {
    /// A repository path violates the path invariants.
    #[error("invalid repository path `{0}`: {1}")]
    InvalidPath(String, &'static str),

    /// A git-style mode integer has no representation in [`FileMode`](crate::FileMode).
    #[error("unsupported file mode 0o{0:o}")]
    UnsupportedMode(u32),

    /// A commit ID is not usable.
    #[error("invalid commit id `{0}`: must not be empty")]
    InvalidCommitId(String),
}
