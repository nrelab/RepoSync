//! Sync engine for RepoSync.
//!
//! [`Syncer`] ties the engine together end to end: it reads a source
//! [`GitRepo`] snapshot, runs a transform pipeline in memory, diffs the result
//! against the destination, and only then commits and pushes. A run whose
//! output matches the destination's HEAD makes zero commits, and any error
//! leaves the destination untouched.

mod error;
mod sync;

pub use error::Error;
pub use sync::{SyncReport, Syncer};
