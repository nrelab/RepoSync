use thiserror::Error;

/// Errors produced by the transformation engine.
#[derive(Debug, Error)]
pub enum Error {
    /// A transform rejected its inputs (paths, globs, collisions).
    #[error("transform `{name}` failed: {message}")]
    Transform { name: String, message: String },

    /// A glob pattern is malformed.
    #[error("invalid glob pattern `{pattern}`: {message}")]
    InvalidPattern {
        pattern: String,
        message: &'static str,
    },

    /// A WASM plugin failed to compile, instantiate, or transform.
    #[error("plugin `{name}` failed: {message}")]
    Plugin { name: String, message: String },

    /// The repository model rejected a value.
    #[error("repository model error: {0}")]
    Core(#[from] reposync_core::Error),
}
