use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::Error;
use crate::policy::Policy;
use crate::source::Destination;
use crate::source::Source;
use crate::transform::TransformNode;
use crate::validate::{self, ValidationError};

/// Top-level `pipeline` section describing the migration itself.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PipelineInfo {
    /// Human-readable name of the pipeline.
    #[serde(default)]
    pub name: Option<String>,
}

/// The root of a pipeline configuration file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigFile {
    /// Pipeline metadata.
    pub pipeline: PipelineInfo,
    /// Where to read the repository from.
    pub source: Source,
    /// Ordered transformations, applied left to right.
    pub transform: Vec<TransformNode>,
    /// Where to write the transformed repository.
    pub destination: Destination,
    /// Optional safety policy applied before pushing.
    #[serde(default)]
    pub policy: Option<Policy>,
}

impl ConfigFile {
    /// Parses a configuration from a YAML string.
    ///
    /// # Errors
    ///
    /// Returns a `serde_yaml` error if the YAML is malformed or does not match
    /// the schema (including unknown keys).
    pub fn from_yaml(yaml: &str) -> Result<Self, serde_yaml::Error> {
        serde_yaml::from_str(yaml)
    }

    /// Loads and parses a configuration file.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] if the file cannot be read, or [`Error::Parse`]
    /// if its contents do not match the schema.
    pub fn load(path: &Path) -> Result<Self, Error> {
        let contents = std::fs::read_to_string(path).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_yaml(&contents).map_err(|source| Error::Parse {
            path: path.to_path_buf(),
            source,
        })
    }

    /// Runs semantic validation, returning all problems found.
    #[must_use]
    pub fn validate(&self) -> Vec<ValidationError> {
        validate::validate(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TransformKind;

    const OK: &str = r"
pipeline:
  name: public-sdk
source:
  type: git
  url: git@github.com:acme/monorepo.git
  ref: main
transform:
  - filter:
      paths:
        - packages/sdk/**
        - LICENSE
  - move:
      from: packages/sdk
      to: .
destination:
  type: git
  url: git@github.com:acme/public-sdk.git
  branch: main
";

    #[test]
    fn parses_valid_config() {
        let config = ConfigFile::from_yaml(OK).unwrap();
        assert_eq!(config.pipeline.name.as_deref(), Some("public-sdk"));
        assert_eq!(config.source.url(), "git@github.com:acme/monorepo.git");
        assert_eq!(
            config.destination.url(),
            "git@github.com:acme/public-sdk.git"
        );
        assert_eq!(config.transform.len(), 2);
        assert_eq!(config.transform[0].kind(), Some(TransformKind::Filter));
        assert_eq!(config.transform[1].kind(), Some(TransformKind::Move));
    }

    #[test]
    fn rejects_unknown_top_level_key() {
        let bad = format!("{OK}extra: value\n");
        assert!(ConfigFile::from_yaml(&bad).is_err());
    }

    #[test]
    fn rejects_missing_required_sections() {
        let bad = "pipeline:\n  name: x\n";
        assert!(ConfigFile::from_yaml(bad).is_err());
    }

    #[test]
    fn yaml_round_trip() {
        let config = ConfigFile::from_yaml(OK).unwrap();
        let yaml = serde_yaml::to_string(&config).unwrap();
        let back = ConfigFile::from_yaml(&yaml).unwrap();
        assert_eq!(back, config);
    }
}
