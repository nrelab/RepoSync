//! Policy engine for RepoSync.
//!
//! Evaluates a [`Policy`](reposync_config::Policy) against a planned
//! [`SnapshotDiff`] and transformed snapshot, producing a
//! [`PolicyReport`] that the CLI and sync engine use to gate pushes.

mod check;
mod error;

pub use check::{check, PolicyReport, PolicyViolation};
pub use error::Error;
