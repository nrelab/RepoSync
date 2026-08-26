use std::fmt;

use serde::{Deserialize, Serialize};

use crate::Error;

/// Identifier of a commit, stored as its hex object ID.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CommitId(String);

impl CommitId {
    /// Creates a commit ID from a non-empty hex string.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidCommitId`] if the ID is empty.
    pub fn new(id: impl Into<String>) -> Result<Self, Error> {
        let inner = id.into();
        if inner.is_empty() {
            return Err(Error::InvalidCommitId(inner));
        }
        Ok(Self(inner))
    }

    /// Returns the hex object ID.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CommitId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Identity of an author or committer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signature {
    /// Display name.
    pub name: String,
    /// Email address.
    pub email: String,
    /// UNIX timestamp in seconds.
    pub time: i64,
    /// Timezone offset from UTC in minutes (east positive).
    pub offset_minutes: i32,
}

impl Signature {
    /// Creates a new signature.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        email: impl Into<String>,
        time: i64,
        offset_minutes: i32,
    ) -> Self {
        Self {
            name: name.into(),
            email: email.into(),
            time,
            offset_minutes,
        }
    }
}

/// A commit's metadata, decoupled from the tree contents (which live in a
/// [`RepositorySnapshot`](crate::RepositorySnapshot)).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Commit {
    /// Commit object ID.
    pub id: CommitId,
    /// Parent commit IDs, in order.
    pub parents: Vec<CommitId>,
    /// Commit message (full text, subject and body).
    pub message: String,
    /// Author signature.
    pub author: Signature,
    /// Committer signature.
    pub committer: Signature,
}

impl Commit {
    /// Creates a commit record.
    #[must_use]
    pub fn new(
        id: CommitId,
        parents: Vec<CommitId>,
        message: impl Into<String>,
        author: Signature,
        committer: Signature,
    ) -> Self {
        Self {
            id,
            parents,
            message: message.into(),
            author,
            committer,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_commit_id() {
        assert!(CommitId::new("").is_err());
        assert!(CommitId::new("abc123").is_ok());
    }

    #[test]
    fn json_round_trip() {
        let commit = Commit::new(
            CommitId::new("deadbeef").unwrap(),
            vec![CommitId::new("cafebabe").unwrap()],
            "subject\n\nbody",
            Signature::new("Alice", "alice@example.com", 1_700_000_000, 120),
            Signature::new("Alice", "alice@example.com", 1_700_000_000, 120),
        );
        let json = serde_json::to_string(&commit).unwrap();
        let back: Commit = serde_json::from_str(&json).unwrap();
        assert_eq!(back, commit);
    }
}
