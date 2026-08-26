use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::blob::Blob;
use crate::commit::CommitId;
use crate::mode::FileMode;
use crate::path::RepoPath;

/// A file in a repository snapshot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    /// Repository path of the file.
    pub path: RepoPath,
    /// File content.
    pub content: Blob,
    /// File mode.
    pub mode: FileMode,
}

impl FileEntry {
    /// Creates a file entry.
    #[must_use]
    pub fn new(path: RepoPath, content: Blob, mode: FileMode) -> Self {
        Self {
            path,
            content,
            mode,
        }
    }

    /// Creates a regular (non-executable) file entry.
    #[must_use]
    pub fn regular(path: RepoPath, content: Blob) -> Self {
        Self::new(path, content, FileMode::File)
    }
}

/// Metadata about the repository a snapshot was taken from.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryMetadata {
    /// The checked-out commit, if known.
    pub head: Option<CommitId>,
    /// The head commit's message, if known.
    pub head_message: Option<String>,
    /// Arbitrary user-defined key/value metadata, set by `metadata` transforms.
    #[serde(default)]
    pub custom: BTreeMap<String, String>,
}

/// An immutable, in-memory view of a repository tree.
///
/// Files are stored in a `BTreeMap` keyed by [`RepoPath`], which guarantees
/// deterministic iteration order — a core requirement for reproducible
/// transformations and diffs.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositorySnapshot {
    /// All files in the snapshot, keyed by path.
    pub files: BTreeMap<RepoPath, FileEntry>,
    /// Repository-level metadata.
    pub metadata: RepositoryMetadata,
}

impl RepositorySnapshot {
    /// Creates an empty snapshot.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a snapshot from an existing file map.
    #[must_use]
    pub fn from_files(files: BTreeMap<RepoPath, FileEntry>) -> Self {
        Self {
            files,
            metadata: RepositoryMetadata::default(),
        }
    }

    /// Number of files in the snapshot.
    #[must_use]
    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// Returns `true` if the snapshot contains no files.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Returns the file at the given path, if present.
    #[must_use]
    pub fn get(&self, path: &RepoPath) -> Option<&FileEntry> {
        self.files.get(path)
    }

    /// Returns a mutable reference to the file at the given path, if present.
    pub fn get_mut(&mut self, path: &RepoPath) -> Option<&mut FileEntry> {
        self.files.get_mut(path)
    }

    /// Returns `true` if a file exists at the given path.
    #[must_use]
    pub fn contains(&self, path: &RepoPath) -> bool {
        self.files.contains_key(path)
    }

    /// Inserts a file entry, replacing any existing entry at the same path.
    pub fn insert(&mut self, entry: FileEntry) -> Option<FileEntry> {
        self.files.insert(entry.path.clone(), entry)
    }

    /// Removes the file at the given path, returning it if present.
    pub fn remove(&mut self, path: &RepoPath) -> Option<FileEntry> {
        self.files.remove(path)
    }

    /// Iterates over `(path, entry)` pairs in deterministic path order.
    pub fn iter(&self) -> impl Iterator<Item = (&RepoPath, &FileEntry)> {
        self.files.iter()
    }

    /// Iterates over file paths in deterministic path order.
    pub fn paths(&self) -> impl Iterator<Item = &RepoPath> {
        self.files.keys()
    }

    /// Returns the repository metadata.
    #[must_use]
    pub const fn metadata(&self) -> &RepositoryMetadata {
        &self.metadata
    }

    /// Returns the repository metadata, mutably.
    pub fn metadata_mut(&mut self) -> &mut RepositoryMetadata {
        &mut self.metadata
    }

    /// Serializes the snapshot to compact JSON.
    ///
    /// # Errors
    ///
    /// Returns a `serde_json` error if serialization fails.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Serializes the snapshot to pretty-printed JSON.
    ///
    /// # Errors
    ///
    /// Returns a `serde_json` error if serialization fails.
    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Deserializes a snapshot from JSON.
    ///
    /// # Errors
    ///
    /// Returns a `serde_json` error if the JSON is invalid or does not match
    /// the snapshot schema.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

impl From<BTreeMap<RepoPath, FileEntry>> for RepositorySnapshot {
    fn from(files: BTreeMap<RepoPath, FileEntry>) -> Self {
        Self::from_files(files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, content: &[u8]) -> FileEntry {
        FileEntry::regular(RepoPath::new(path).unwrap(), Blob::from_bytes(content))
    }

    #[test]
    fn insert_get_remove() {
        let mut snapshot = RepositorySnapshot::new();
        assert!(snapshot.is_empty());

        snapshot.insert(file("a.txt", b"a"));
        snapshot.insert(file("b/c.txt", b"c"));
        assert_eq!(snapshot.len(), 2);

        let a = RepoPath::new("a.txt").unwrap();
        assert!(snapshot.contains(&a));
        assert_eq!(snapshot.get(&a).unwrap().content.content(), b"a");

        let removed = snapshot.remove(&a).unwrap();
        assert_eq!(removed.path, a);
        assert_eq!(snapshot.len(), 1);
    }

    #[test]
    fn json_round_trip() {
        let mut snapshot = RepositorySnapshot::new();
        snapshot.insert(file("README.md", b"# hello"));
        snapshot.insert(file("src/lib.rs", b"pub fn f() {}"));
        snapshot.metadata.head_message = Some("initial commit".into());

        let json = snapshot.to_json_pretty().unwrap();
        let back = RepositorySnapshot::from_json(&json).unwrap();
        assert_eq!(back, snapshot);
    }

    #[test]
    fn iteration_is_deterministic() {
        let mut snapshot = RepositorySnapshot::new();
        snapshot.insert(file("z.txt", b"z"));
        snapshot.insert(file("a.txt", b"a"));
        snapshot.insert(file("m/x.txt", b"x"));

        let paths: Vec<&str> = snapshot.paths().map(RepoPath::as_str).collect();
        assert_eq!(paths, vec!["a.txt", "m/x.txt", "z.txt"]);
        assert_eq!(paths, {
            let mut sorted = paths.clone();
            sorted.sort_unstable();
            sorted
        });
    }
}
