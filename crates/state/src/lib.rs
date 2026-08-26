//! Persistent sync state for RepoSync.
//!
//! The state database answers one question: *which destination commit
//! corresponds to which source commit?* Storing this mapping lets a repeated
//! `migrate` skip source commits it has already exported, and makes the sync
//! loop-prevention of later milestones (M12) possible.
//!
//! The on-disk format is a SQLite database (`state.db`). The plan's earlier
//! `.sync/state.json` MVP is intentionally skipped — SQLite is the shipped
//! form and the schema is small enough to bootstrap directly.
//!
//! Schema
//! ------
//! ```text
//! commit_mapping(
//!     source_commit      TEXT PRIMARY KEY,
//!     destination_commit TEXT NOT NULL,
//!     timestamp          INTEGER NOT NULL
//! )
//! ```

use std::path::Path;

use reposync_core::CommitId;
use serde::{Deserialize, Serialize};

/// Errors that can occur while opening or updating the state database.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A filesystem error while preparing the database file or its directory.
    #[error("state io error: {0}")]
    Io(#[from] std::io::Error),
    /// An error from the underlying SQLite engine.
    #[error("state database error: {0}")]
    Db(#[from] rusqlite::Error),
}

/// Convenience result alias for the state crate.
pub type Result<T> = std::result::Result<T, Error>;

/// A single source→destination commit mapping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitMapping {
    /// The source commit id this row was produced from.
    pub source_commit: String,
    /// The destination commit id the source was exported to.
    pub destination_commit: String,
    /// Unix epoch seconds when the mapping was recorded.
    pub timestamp: i64,
}

/// A handle to the persistent sync state.
///
/// Open (or create) a database with [`State::open`], then query and record
/// commit mappings. All operations are scoped to the `commit_mapping` table.
#[derive(Debug)]
pub struct State {
    conn: rusqlite::Connection,
}

impl State {
    /// Open the database at `path`, creating it (and its directory) if missing.
    ///
    /// # Errors
    ///
    /// Returns an error if the directory cannot be created or SQLite fails to
    /// open or initialize the schema.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let conn = rusqlite::Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS commit_mapping (
                source_commit      TEXT PRIMARY KEY,
                destination_commit TEXT NOT NULL,
                timestamp          INTEGER NOT NULL
            )",
        )?;
        Ok(Self { conn })
    }

    /// Record (or update) the destination commit produced from `source`.
    ///
    /// Because `source_commit` is the primary key, recording the same source
    /// twice replaces the previous row rather than creating a duplicate.
    ///
    /// # Errors
    ///
    /// Returns an error if the insert fails.
    pub fn record(&self, source: &CommitId, destination: &CommitId) -> Result<()> {
        let timestamp = now();
        self.conn.execute(
            "INSERT OR REPLACE INTO commit_mapping (source_commit, destination_commit, timestamp)
             VALUES (?1, ?2, ?3)",
            rusqlite::params![source.as_str(), destination.as_str(), timestamp],
        )?;
        Ok(())
    }

    /// Whether `source` has already been mapped to a destination commit.
    ///
    /// # Errors
    ///
    /// Returns an error if the query fails.
    pub fn has_source(&self, source: &CommitId) -> Result<bool> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM commit_mapping WHERE source_commit = ?1",
            rusqlite::params![source.as_str()],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    /// The destination commit previously recorded for `source`, if any.
    ///
    /// # Errors
    ///
    /// Returns an error if the query fails for a reason other than "no row".
    pub fn destination_for(&self, source: &CommitId) -> Result<Option<String>> {
        match self.conn.query_row(
            "SELECT destination_commit FROM commit_mapping WHERE source_commit = ?1",
            rusqlite::params![source.as_str()],
            |row| row.get::<_, String>(0),
        ) {
            Ok(value) => Ok(Some(value)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(error) => Err(Error::Db(error)),
        }
    }

    /// All recorded mappings, ordered oldest first.
    ///
    /// # Errors
    ///
    /// Returns an error if the query fails.
    pub fn mappings(&self) -> Result<Vec<CommitMapping>> {
        let mut statement = self.conn.prepare(
            "SELECT source_commit, destination_commit, timestamp
             FROM commit_mapping ORDER BY timestamp ASC",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(CommitMapping {
                source_commit: row.get(0)?,
                destination_commit: row.get(1)?,
                timestamp: row.get(2)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }
}

/// Current time as Unix epoch seconds.
fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| i64::try_from(duration.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use reposync_core::CommitId;

    fn tmp_db() -> std::path::PathBuf {
        let dir = tempfile::TempDir::new().unwrap().into_path();
        dir.join("state.db")
    }

    #[test]
    fn records_and_looks_up_mapping() {
        let state = State::open(&tmp_db()).unwrap();
        let src = CommitId::new("a1b2c3").unwrap();
        let dst = CommitId::new("d4e5f6").unwrap();

        assert!(!state.has_source(&src).unwrap());
        assert!(state.destination_for(&src).unwrap().is_none());

        state.record(&src, &dst).unwrap();

        assert!(state.has_source(&src).unwrap());
        assert_eq!(state.destination_for(&src).unwrap(), Some("d4e5f6".into()));
    }

    #[test]
    fn record_is_idempotent_per_source() {
        let state = State::open(&tmp_db()).unwrap();
        let src = CommitId::new("src1").unwrap();
        state.record(&src, &CommitId::new("dst1").unwrap()).unwrap();
        state.record(&src, &CommitId::new("dst2").unwrap()).unwrap();

        let mappings = state.mappings().unwrap();
        assert_eq!(mappings.len(), 1, "expected exactly one row per source");
        assert_eq!(mappings[0].destination_commit, "dst2");
    }

    #[test]
    fn mappings_returns_all_rows_oldest_first() {
        let state = State::open(&tmp_db()).unwrap();
        state
            .record(&CommitId::new("s1").unwrap(), &CommitId::new("d1").unwrap())
            .unwrap();
        state
            .record(&CommitId::new("s2").unwrap(), &CommitId::new("d2").unwrap())
            .unwrap();

        let mappings = state.mappings().unwrap();
        assert_eq!(mappings.len(), 2);
        assert_eq!(mappings[0].source_commit, "s1");
        assert_eq!(mappings[0].destination_commit, "d1");
        assert_eq!(mappings[1].source_commit, "s2");
    }
}

