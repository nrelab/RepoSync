//! Migration and synchronization engine for RepoSync.
//!
//! Orchestrates history migration (M11), bidirectional sync with loop
//! prevention (M12), and state persistence such as the commit mapping database
//! (M10). See PLAN.md for milestone details.

mod error;
mod history;
mod sync;

pub use error::Error;
pub use history::{replay_history, HistoryReport};
pub use sync::{sync, Conflict, SyncReport, SyncStrategy};
