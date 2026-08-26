use std::collections::BTreeMap;

use reposync_core::{Blob, FileEntry, RepoPath, RepositorySnapshot};

/// Builds a snapshot from `(path, content bytes)` pairs for use in tests.
#[cfg(test)]
pub(crate) fn snapshot_from(entries: &[(&str, &[u8])]) -> RepositorySnapshot {
    let mut s = RepositorySnapshot::new();
    for (path, content) in entries {
        s.insert(FileEntry::regular(
            RepoPath::new(*path).unwrap(),
            Blob::from_bytes(content.to_vec()),
        ));
    }
    s
}
