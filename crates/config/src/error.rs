use std::path::PathBuf;

use thiserror::Error;

/// Errors produced while loading or validating a configuration.
#[derive(Debug, Error)]
pub enum Error {
    /// The configuration file could not be read.
    #[error("failed to read {path}: {source}")]
    Io {
        /// Path of the configuration file.
        path: PathBuf,
        /// Underlying I/O error.
        source: std::io::Error,
    },

    /// The configuration file could not be parsed into the schema.
    #[error("failed to parse {path}: {source}")]
    Parse {
        /// Path of the configuration file.
        path: PathBuf,
        /// Underlying YAML/serde error.
        source: serde_yaml::Error,
    },
}
