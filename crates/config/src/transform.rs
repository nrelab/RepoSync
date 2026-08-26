use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// All transformation kinds known to the config schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransformKind {
    Filter,
    Delete,
    Copy,
    Move,
    Rename,
    Replace,
    RegexReplace,
    StripPrefix,
    Prepend,
    Append,
    Patch,
    Metadata,
    CommitMessage,
    AuthorMapping,
    DependencyRewrite,
    ImportRewrite,
    Custom,
}

impl TransformKind {
    /// Returns the YAML key used to spell this transform.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Filter => "filter",
            Self::Delete => "delete",
            Self::Copy => "copy",
            Self::Move => "move",
            Self::Rename => "rename",
            Self::Replace => "replace",
            Self::RegexReplace => "regex_replace",
            Self::StripPrefix => "strip_prefix",
            Self::Prepend => "prepend",
            Self::Append => "append",
            Self::Patch => "patch",
            Self::Metadata => "metadata",
            Self::CommitMessage => "commit_message",
            Self::AuthorMapping => "author_mapping",
            Self::DependencyRewrite => "dependency_rewrite",
            Self::ImportRewrite => "import_rewrite",
            Self::Custom => "custom",
        }
    }

    /// Whether the transformation engine can currently execute this kind.
    ///
    /// Transforms that are recognized by the schema but not yet executable by
    /// the engine are rejected during validation with the accompanying reason.
    #[must_use]
    pub const fn engine_support(self) -> EngineSupport {
        match self {
            Self::Filter
            | Self::Delete
            | Self::Copy
            | Self::Move
            | Self::Rename
            | Self::Replace
            | Self::StripPrefix
            |             Self::DependencyRewrite
            | Self::ImportRewrite
            | Self::AuthorMapping
            | Self::CommitMessage
            | Self::Custom => EngineSupport::Supported,
            Self::RegexReplace | Self::Prepend | Self::Append | Self::Patch | Self::Metadata => {
                EngineSupport::Supported
            }
        }
    }
}

impl std::fmt::Display for TransformKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Whether the engine can execute a transform kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineSupport {
    /// The engine can execute this transform.
    Supported,
    /// The transform is recognized but not yet executable.
    NotYetImplemented(&'static str),
}

macro_rules! args {
    ($name:ident { $( $(#[$meta:meta])* $field:ident : $ty:ty ),* $(,)? }) => {
        /// Arguments for a single transform node.
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(deny_unknown_fields)]
        pub struct $name {
            $(
                #[doc = "Transform argument."]
                $(#[$meta])*
                pub $field: $ty,
            )*
        }
    };
}

args!(FilterArgs { paths: Vec<String> });
args!(DeleteArgs { paths: Vec<String> });
args!(CopyArgs {
    from: String,
    to: String,
});
args!(MoveArgs {
    from: String,
    to: String,
});
args!(RenameArgs {
    from: String,
    to: String,
});
args!(ReplaceArgs {
    file: String,
    replacements: BTreeMap<String, String>,
});
args!(RegexReplaceArgs {
    files: Vec<String>,
    pattern: String,
    replacement: String,
});
args!(StripPrefixArgs { path: String });
args!(PrependArgs {
    files: Vec<String>,
    content: String,
});
args!(AppendArgs {
    files: Vec<String>,
    content: String,
});
args!(PatchArgs {
    file: String,
    patch: String,
});
args!(MetadataArgs {
    key: String,
    value: String,
});
args!(CommitMessageArgs { message: String });
args!(AuthorMappingArgs {
    mapping: BTreeMap<String, String>,
});
args!(DependencyRewriteArgs {
    package_rename: BTreeMap<String, String>,
    workspace_version: Option<String>,
});
args!(ImportRewriteArgs {
    renames: BTreeMap<String, String>,
});
args!(CustomArgs {
    name: String,
    path: String,
    #[serde(default)]
    args: BTreeMap<String, serde_yaml::Value>,
});

/// A single step in a pipeline. Exactly one transform must be set; validation
/// enforces the one-of invariant.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TransformNode {
    /// Keep only the given paths.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<FilterArgs>,
    /// Delete the given paths.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delete: Option<DeleteArgs>,
    /// Copy a path prefix to another location.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub copy: Option<CopyArgs>,
    /// Move a path prefix to another location.
    #[serde(rename = "move", skip_serializing_if = "Option::is_none")]
    pub r#move: Option<MoveArgs>,
    /// Rename a path prefix.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rename: Option<RenameArgs>,
    /// Replace substrings within a file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replace: Option<ReplaceArgs>,
    /// Replace regex matches within files.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub regex_replace: Option<RegexReplaceArgs>,
    /// Strip a leading path prefix.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strip_prefix: Option<StripPrefixArgs>,
    /// Prepend content to files.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prepend: Option<PrependArgs>,
    /// Append content to files.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub append: Option<AppendArgs>,
    /// Apply a patch to a file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch: Option<PatchArgs>,
    /// Set repository metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<MetadataArgs>,
    /// Override the commit message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_message: Option<CommitMessageArgs>,
    /// Remap authors (email -> email).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author_mapping: Option<AuthorMappingArgs>,
    /// Rewrite `package.json` manifests (name + workspace deps) on extraction.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependency_rewrite: Option<DependencyRewriteArgs>,
    /// Rewrite module specifiers in source files (AST-lite import rewrite).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub import_rewrite: Option<ImportRewriteArgs>,
    /// Run a named plugin.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom: Option<CustomArgs>,
}

impl TransformNode {
    /// Returns the kind of this node, if exactly one transform is set.
    #[must_use]
    pub fn kind(&self) -> Option<TransformKind> {
        if self.filter.is_some() {
            Some(TransformKind::Filter)
        } else if self.delete.is_some() {
            Some(TransformKind::Delete)
        } else if self.copy.is_some() {
            Some(TransformKind::Copy)
        } else if self.r#move.is_some() {
            Some(TransformKind::Move)
        } else if self.rename.is_some() {
            Some(TransformKind::Rename)
        } else if self.replace.is_some() {
            Some(TransformKind::Replace)
        } else if self.regex_replace.is_some() {
            Some(TransformKind::RegexReplace)
        } else if self.strip_prefix.is_some() {
            Some(TransformKind::StripPrefix)
        } else if self.prepend.is_some() {
            Some(TransformKind::Prepend)
        } else if self.append.is_some() {
            Some(TransformKind::Append)
        } else if self.patch.is_some() {
            Some(TransformKind::Patch)
        } else if self.metadata.is_some() {
            Some(TransformKind::Metadata)
        } else if self.commit_message.is_some() {
            Some(TransformKind::CommitMessage)
        } else if self.author_mapping.is_some() {
            Some(TransformKind::AuthorMapping)
        } else if self.dependency_rewrite.is_some() {
            Some(TransformKind::DependencyRewrite)
        } else if self.import_rewrite.is_some() {
            Some(TransformKind::ImportRewrite)
        } else if self.custom.is_some() {
            Some(TransformKind::Custom)
        } else {
            None
        }
    }

    /// Returns the number of transforms set on this node.
    #[must_use]
    pub fn kind_count(&self) -> usize {
        let present = [
            self.filter.is_some(),
            self.delete.is_some(),
            self.copy.is_some(),
            self.r#move.is_some(),
            self.rename.is_some(),
            self.replace.is_some(),
            self.regex_replace.is_some(),
            self.strip_prefix.is_some(),
            self.prepend.is_some(),
            self.append.is_some(),
            self.patch.is_some(),
            self.metadata.is_some(),
            self.commit_message.is_some(),
            self.author_mapping.is_some(),
            self.dependency_rewrite.is_some(),
            self.import_rewrite.is_some(),
            self.custom.is_some(),
        ];
        present.into_iter().filter(|present| *present).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_kind() {
        let node = TransformNode {
            filter: Some(FilterArgs {
                paths: vec!["a".into()],
            }),
            ..TransformNode::default()
        };
        assert_eq!(node.kind(), Some(TransformKind::Filter));
        assert_eq!(node.kind_count(), 1);
    }

    #[test]
    fn move_keyword_round_trip() {
        let node = TransformNode {
            r#move: Some(MoveArgs {
                from: "packages/sdk".into(),
                to: ".".into(),
            }),
            ..TransformNode::default()
        };
        let yaml = serde_yaml::to_string(&node).unwrap();
        assert!(yaml.contains("move:"));
        let back: TransformNode = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(back, node);
    }

    #[test]
    fn engine_support_gates() {
        assert_eq!(
            TransformKind::Filter.engine_support(),
            EngineSupport::Supported
        );
        assert_eq!(
            TransformKind::Replace.engine_support(),
            EngineSupport::Supported
        );
        assert_eq!(
            TransformKind::DependencyRewrite.engine_support(),
            EngineSupport::Supported
        );
        assert_eq!(
            TransformKind::RegexReplace.engine_support(),
            EngineSupport::Supported
        );
        assert_eq!(
            TransformKind::Prepend.engine_support(),
            EngineSupport::Supported
        );
        assert_eq!(
            TransformKind::Append.engine_support(),
            EngineSupport::Supported
        );
        assert_eq!(
            TransformKind::Patch.engine_support(),
            EngineSupport::Supported
        );
        assert_eq!(
            TransformKind::Metadata.engine_support(),
            EngineSupport::Supported
        );
        assert_eq!(
            TransformKind::Custom.engine_support(),
            EngineSupport::Supported
        );
    }
}
