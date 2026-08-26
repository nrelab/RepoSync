//! Diff engine for RepoSync.
//!
//! [`diff`] computes a path/content-level [`SnapshotDiff`] between two
//! snapshots: added, modified, and removed files, plus rename detection so
//! moves surface as renames rather than delete+add pairs. Content identity is
//! decided by each [`Blob`](reposync_core::Blob)'s content-addressed hash, so
//! no content is read twice. Results are deterministic for identical inputs.
//!
//! The diff feeds the CLI dry-run report, policy checks, and (later) sync.

mod engine;
mod types;

pub use engine::diff;
pub use types::{DiffEntry, RenameEntry, SnapshotDiff};
