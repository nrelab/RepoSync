use reposync_core::{BlobId, FileMode, RepoPath};

/// A file that was added, removed, or modified between two snapshots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffEntry {
    /// Repository path of the file.
    pub path: RepoPath,
    /// Content hash before the change, if the file existed in the old snapshot.
    pub old_hash: Option<BlobId>,
    /// Content hash after the change, if the file exists in the new snapshot.
    pub new_hash: Option<BlobId>,
    /// Mode before the change, if the file existed in the old snapshot.
    pub old_mode: Option<FileMode>,
    /// Mode after the change, if the file exists in the new snapshot.
    pub new_mode: Option<FileMode>,
}

/// A file whose path changed while content and mode stayed identical.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenameEntry {
    /// Old path.
    pub from: RepoPath,
    /// New path.
    pub to: RepoPath,
    /// Content hash, identical on both sides.
    pub hash: BlobId,
    /// File mode, identical on both sides.
    pub mode: FileMode,
}

/// Path/content-level difference between two snapshots.
///
/// Rename detection pairs removed files with added files of identical content
/// and mode, so moves surface as renames rather than delete+add pairs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SnapshotDiff {
    /// Files added in the new snapshot.
    pub added: Vec<DiffEntry>,
    /// Files present in both snapshots with different content or mode.
    pub modified: Vec<DiffEntry>,
    /// Files removed from the old snapshot.
    pub removed: Vec<DiffEntry>,
    /// Files renamed (identical content and mode) between the snapshots.
    pub renamed: Vec<RenameEntry>,
}

impl SnapshotDiff {
    /// Returns `true` when no files changed between the snapshots.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.added.is_empty()
            && self.modified.is_empty()
            && self.removed.is_empty()
            && self.renamed.is_empty()
    }

    /// Total number of changed files (each rename counts once).
    #[must_use]
    pub fn changes(&self) -> usize {
        self.added.len() + self.modified.len() + self.removed.len() + self.renamed.len()
    }
}
