//! Declarative YAML pipeline configuration for RepoSync.
//!
//! The DSL describes a `source` repository, an ordered list of `transform`
//! nodes, a `destination` repository, and an optional `policy`:
//!
//! ```yaml
//! pipeline:
//!   name: public-sdk
//! source:
//!   type: git
//!   url: git@github.com:org/internal.git
//!   ref: main
//! transform:
//!   - filter: { paths: [packages/sdk/**, LICENSE] }
//!   - move: { from: packages/sdk, to: . }
//! destination:
//!   type: git
//!   url: git@github.com:org/sdk.git
//!   branch: main
//! ```
//!
//! Parsing is strict: unknown top-level keys, unknown transform names, and
//! unknown fields inside providers or transforms are rejected. Semantic
//! validation happens after parsing via [`validate`].

mod config;
mod error;
mod policy;
mod source;
mod transform;
mod validate;

pub use config::{ConfigFile, PipelineInfo};
pub use error::Error;
pub use policy::Policy;
pub use source::{is_valid_git_url, normalize_url, Destination, Source};
pub use transform::{
    AppendArgs, AuthorMappingArgs, CommitMessageArgs, CopyArgs, CustomArgs, DeleteArgs,
    EngineSupport, FilterArgs, MetadataArgs, MoveArgs, PatchArgs, PrependArgs, RegexReplaceArgs,
    RenameArgs, ReplaceArgs, StripPrefixArgs, TransformKind, TransformNode,
};
pub use validate::{validate, ValidationError};
