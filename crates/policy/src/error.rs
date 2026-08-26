use thiserror::Error;

/// Errors produced by the policy engine.
#[derive(Debug, Error)]
pub enum Error {
    /// A denied path pattern matched a file in the planned output.
    #[error("policy denied path `{path}` (pattern: `{pattern}`)")]
    DeniedPath { path: String, pattern: String },

    /// A require-review path matched a file in the planned output.
    #[error("policy requires review for `{path}` (pattern: `{pattern}`)")]
    RequireReview { path: String, pattern: String },

    /// The planned run would delete more files than allowed.
    #[error("policy exceeded max_deleted_files: {actual} > {limit}")]
    TooManyDeleted { limit: u64, actual: u64 },

    /// A deny-pattern glob is malformed.
    #[error("invalid policy glob `{pattern}`: {message}")]
    InvalidGlob { pattern: String, message: &'static str },
}
