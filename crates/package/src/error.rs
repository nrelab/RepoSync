use thiserror::Error;

/// Errors from package resolution, fetch and cache operations.
///
/// Each variant maps to a CLI exit code per `PRODUCT.md#10`.
#[derive(Debug, Error)]
pub enum PackageError {
    #[error("invalid package spec '{spec}': {reason}")]
    InvalidSpec { spec: String, reason: String },

    #[error("unsupported registry '{registry}'")]
    UnsupportedRegistry { registry: String },

    #[error("package '{name}' not found in registry '{registry}'")]
    NotFound { registry: String, name: String },

    #[error("version '{version}' not found for '{name}'")]
    VersionNotFound { name: String, version: String },

    #[error("failed to fetch {url}: {cause}")]
    Network { url: String, cause: String },

    #[error("integrity check failed for {name}@{version}")]
    Integrity { name: String, version: String },

    #[error("failed to extract {name}@{version}: {cause}")]
    Extraction { name: String, version: String, cause: String },

    #[error("cache I/O error at {path}: {cause}")]
    Io { path: String, cause: String },

    #[error("permission denied writing cache at {path}: {cause}. hint: set $REPOSYNC_CACHE to a writable directory")]
    Permission { path: String, cause: String },
}

impl PackageError {
    /// CLI exit code per PRODUCT.md#10.
    #[must_use]
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::InvalidSpec { .. } | Self::UnsupportedRegistry { .. } => 2,
            Self::NotFound { .. } | Self::VersionNotFound { .. } => 3,
            Self::Network { .. } => 4,
            Self::Integrity { .. } => 5,
            Self::Io { .. } | Self::Extraction { .. } => 6,
            Self::Permission { .. } => 6,
        }
    }

    /// Human hint for unsupported registry errors.
    #[must_use]
    pub fn supported_registries() -> &'static str {
        "supported registries: npm, pypi"
    }
}
