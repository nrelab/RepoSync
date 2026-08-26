use std::collections::{BTreeMap, BTreeSet};

use reposync_core::{BlobId, FileEntry, FileMode, RepoPath, RepositorySnapshot};

use crate::types::{DiffEntry, RenameEntry, SnapshotDiff};

/// Compute the path/content-level difference between two snapshots.
///
/// Rename detection matches removed files to added files with the same content
/// hash and mode, pairing them in sorted path order, so the result is
/// deterministic for identical inputs.
#[must_use]
pub fn diff(old: &RepositorySnapshot, new: &RepositorySnapshot) -> SnapshotDiff {
    let old_paths: BTreeSet<RepoPath> = old.files.keys().cloned().collect();
    let new_paths: BTreeSet<RepoPath> = new.files.keys().cloned().collect();
    let added_paths: BTreeSet<RepoPath> = new_paths.difference(&old_paths).cloned().collect();
    let removed_paths: BTreeSet<RepoPath> = old_paths.difference(&new_paths).cloned().collect();

    let mut modified = Vec::new();
    for path in old_paths.intersection(&new_paths) {
        let old_entry = &old.files[path];
        let new_entry = &new.files[path];
        if old_entry.content.hash() != new_entry.content.hash() || old_entry.mode != new_entry.mode {
            modified.push(entry_from(path, Some(old_entry), Some(new_entry)));
        }
    }

    let added_by_key: BTreeMap<(BlobId, FileMode), Vec<RepoPath>> = bucket(&added_paths, new);
    let removed_by_key: BTreeMap<(BlobId, FileMode), Vec<RepoPath>> = bucket(&removed_paths, old);

    let mut renamed = Vec::new();
    let mut consumed_added = BTreeSet::new();
    let mut consumed_removed = BTreeSet::new();
    for (key, added) in &added_by_key {
        let Some(removed) = removed_by_key.get(key) else {
            continue;
        };
        for (add, rem) in added.iter().zip(removed) {
            consumed_added.insert(add.clone());
            consumed_removed.insert(rem.clone());
            renamed.push(RenameEntry {
                from: rem.clone(),
                to: add.clone(),
                hash: key.0,
                mode: key.1,
            });
        }
    }

    let added: Vec<DiffEntry> = added_paths
        .difference(&consumed_added)
        .map(|path| entry_from(path, None, Some(&new.files[path])))
        .collect();
    let removed: Vec<DiffEntry> = removed_paths
        .difference(&consumed_removed)
        .map(|path| entry_from(path, Some(&old.files[path]), None))
        .collect();

    SnapshotDiff {
        added,
        modified,
        removed,
        renamed,
    }
}

/// Group paths by their entry's content hash and mode.
fn bucket(paths: &BTreeSet<RepoPath>, snapshot: &RepositorySnapshot) -> BTreeMap<(BlobId, FileMode), Vec<RepoPath>> {
    let mut buckets: BTreeMap<(BlobId, FileMode), Vec<RepoPath>> = BTreeMap::new();
    for path in paths {
        let entry = &snapshot.files[path];
        buckets
            .entry((entry.content.hash(), entry.mode))
            .or_default()
            .push(path.clone());
    }
    buckets
}

/// Build a [`DiffEntry`] from the old and/or new file entries at a path.
fn entry_from(path: &RepoPath, old: Option<&FileEntry>, new: Option<&FileEntry>) -> DiffEntry {
    DiffEntry {
        path: path.clone(),
        old_hash: old.map(|entry| entry.content.hash()),
        new_hash: new.map(|entry| entry.content.hash()),
        old_mode: old.map(|entry| entry.mode),
        new_mode: new.map(|entry| entry.mode),
    }
}
