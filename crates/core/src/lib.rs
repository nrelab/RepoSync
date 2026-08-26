//! Core repository snapshot model for RepoSync.
//!
//! All transformations operate on an in-memory [`RepositorySnapshot`] rather
//! than directly on a working tree. The model provides deterministic path
//! ordering (a `BTreeMap` keyed by `RepoPath`), content blobs with stable
//! SHA-256 hashes, file modes, and commit metadata.
//!
//! The crate is the heart of the engine: milestones M1+ of PLAN.md build on it.
//!
//! # Modules
//!
//! - [`path::RepoPath`] — validated, `/`-separated relative repository paths.
//! - [`blob::Blob`] / [`blob::BlobId`] — file content with content-addressed IDs.
//! - [`mode::FileMode`] — file/executable/symlink/gitlink mode.
//! - [`commit`] — commit IDs, signatures, and commit metadata.
//! - [`snapshot`] — `FileEntry`, `RepositoryMetadata`, `RepositorySnapshot`.

mod blob;
mod commit;
mod error;
mod mode;
mod path;
mod snapshot;

pub use blob::{Blob, BlobId};
pub use commit::{Commit, CommitId, Signature};
pub use error::Error;
pub use mode::FileMode;
pub use path::RepoPath;
pub use snapshot::{FileEntry, RepositoryMetadata, RepositorySnapshot};
