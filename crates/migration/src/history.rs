//! History migration for RepoSync.
//!
//! [`replay_history`] replays a source repository's commit history into a
//! destination repository, applying the transform pipeline to each commit's
//! tree in turn. Commits are processed oldest-first so every rewritten commit
//! can point at its already-rewritten parents, preserving the original branch
//! shape (for linear and merge histories alike). Author and committer
//! identities are carried over from the source commits.

use std::collections::BTreeMap;

use reposync_core::CommitId;
use reposync_git::{mark_generated, CommitSpec, GitRepo};
use reposync_state::State;
use reposync_transform::{Runner, Transformation};

use crate::Error;

/// Outcome of a history replay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryReport {
    /// Number of source commits that were rewritten and committed to `dest`.
    pub commits_replayed: usize,
    /// Number of source commits that were already mapped (incremental skip).
    pub commits_skipped: usize,
    /// The id of the last destination commit written, if any.
    pub last_commit: Option<CommitId>,
}

/// Replay the source commit history into `dest`, applying `transforms` to each
/// commit's tree in turn.
///
/// Commits are processed oldest-first so that each rewritten commit can point
/// at its already-rewritten parents. The source→destination mapping is recorded
/// in `state` (when provided) so repeated runs are incremental: already-mapped
/// commits are skipped and their recorded destination commit is reused as the
/// parent of subsequent commits.
///
/// Author and committer identities are carried over from the source commit,
/// unless remapped via `author_map` (a `source author email -> destination
/// author email` map). When `message_prefix` is set, it is prepended to each
/// rewritten commit message (after which the generated marker is appended).
///
/// # Errors
///
/// Returns the first git, state, or transform error.
pub fn replay_history(
    source: &GitRepo,
    dest: &GitRepo,
    transforms: &[&dyn Transformation],
    state: Option<&State>,
    author_map: Option<&BTreeMap<String, String>>,
    message_prefix: Option<&str>,
) -> Result<HistoryReport, Error> {
    let history = source.history(None)?; // newest-first, topological order
    let ordered: Vec<CommitId> = history.into_iter().rev().collect(); // oldest-first

    let mut map: BTreeMap<CommitId, CommitId> = BTreeMap::new();
    let mut report = HistoryReport {
        commits_replayed: 0,
        commits_skipped: 0,
        last_commit: None,
    };

    for id in ordered {
        // Incremental skip: reuse the previously recorded destination commit.
        if let Some(state) = state {
            if let Some(dest_id_str) = state.destination_for(&id)? {
                if let Ok(dest_id) = CommitId::new(dest_id_str) {
                    map.insert(id.clone(), dest_id);
                    report.commits_skipped += 1;
                    continue;
                }
            }
        }

        let snapshot = source.snapshot_at_ref(id.as_str())?;
        let run = Runner::new().run(snapshot, transforms)?;
        let info = source.commit_info(&id)?;

        let base_message = match message_prefix {
            Some(prefix) => format!("{prefix}{}", info.message),
            None => info.message,
        };

        let mut author = info.author;
        let mut committer = info.committer;
        if let Some(map_ref) = author_map {
            if let Some(email) = map_ref.get(&author.email) {
                author.email.clone_from(email);
            }
            if let Some(email) = map_ref.get(&committer.email) {
                committer.email.clone_from(email);
            }
        }

        let spec = CommitSpec {
            message: mark_generated(&base_message, &id),
            author: Some(author),
            committer: Some(committer),
        };

        // Resolve this commit's source parents to their rewritten destinations.
        // For a linear history this is zero or one commit; merges yield several.
        let dest_parents: Vec<CommitId> = info
            .parents
            .iter()
            .filter_map(|parent| map.get(parent).cloned())
            .collect();

        let dest_id = dest.write_commit_with_parent_ids(&run.snapshot, &spec, &dest_parents)?;
        map.insert(id.clone(), dest_id.clone());
        if let Some(state) = state {
            state.record(&id, &dest_id)?;
        }
        report.commits_replayed += 1;
        report.last_commit = Some(dest_id);
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use reposync_core::{Blob, FileEntry, RepoPath, RepositorySnapshot, Signature};
    use reposync_transform::{Filter, Transformation};

    fn commit(repo: &GitRepo, files: &[(&str, &[u8])], author: &str) {
        repo.set_identity("ignored", "ignored@example.com").unwrap();
        let mut snapshot = RepositorySnapshot::new();
        for (path, content) in files {
            snapshot.insert(FileEntry::regular(
                RepoPath::new(*path).unwrap(),
                Blob::from_bytes(*content),
            ));
        }
        let signature = Signature::new(author, format!("{author}@example.com"), 1_700_000_000, 0);
        let spec = CommitSpec {
            message: format!("commit by {author}"),
            author: Some(signature.clone()),
            committer: Some(signature),
        };
        repo.write_commit(&snapshot, &spec).unwrap();
    }

    fn history_ids(repo: &GitRepo) -> Vec<CommitId> {
        // newest-first; reverse to oldest-first.
        repo.history(None).unwrap().into_iter().rev().collect()
    }

    #[test]
    fn replay_preserves_parent_chain_and_authors() {
        let root = tempfile::TempDir::new().unwrap();
        let source = GitRepo::init(root.path().join("source")).unwrap();
        let dest = GitRepo::init(root.path().join("dest")).unwrap();

        commit(&source, &[("a.txt", b"a")], "Alice");
        commit(&source, &[("a.txt", b"a"), ("b.txt", b"b")], "Bob");
        commit(
            &source,
            &[("a.txt", b"a"), ("b.txt", b"b"), ("c.txt", b"c")],
            "Carol",
        );
        commit(
            &source,
            &[("a.txt", b"a"), ("b.txt", b"b"), ("c.txt", b"c"), ("d.txt", b"d")],
            "Dan",
        );

        let source_head = source.head_commit_id().unwrap().unwrap();
        let transforms: Vec<Box<dyn Transformation>> = vec![Box::new(Filter::new(["**"]))];
        let transform_refs: Vec<&dyn Transformation> = transforms.iter().map(|b| &**b).collect();

        let report = replay_history(&source, &dest, &transform_refs, None, None, None).unwrap();
        assert_eq!(report.commits_replayed, 4);
        assert_eq!(report.commits_skipped, 0);

        // Dest has 4 commits, oldest-first, forming a valid parent chain.
        let ids = history_ids(&dest);
        assert_eq!(ids.len(), 4, "expected 4 commits in dest");
        for window in ids.windows(2) {
            let child = dest.commit_info(&window[1]).unwrap();
            assert_eq!(child.parents, vec![window[0].clone()]);
        }

        // Authors carried over.
        let dest_info: Vec<_> = ids.iter().map(|id| dest.commit_info(id).unwrap()).collect();
        let authors: Vec<&str> = dest_info.iter().map(|c| c.author.name.as_str()).collect();
        assert_eq!(authors, vec!["Alice", "Bob", "Carol", "Dan"]);

        // Verification: dest head tree matches snapshot-mode output at source HEAD.
        let source_snapshot = source.snapshot_at_ref(source_head.as_str()).unwrap();
        let output = Runner::new()
            .run(source_snapshot, &transform_refs)
            .unwrap()
            .snapshot;
        let dest_head_snapshot = dest.head_snapshot().unwrap();
        assert_eq!(output.files, dest_head_snapshot.files);
    }

    #[test]
    fn replay_is_incremental_with_state() {
        let root = tempfile::TempDir::new().unwrap();
        let source = GitRepo::init(root.path().join("source")).unwrap();
        let dest = GitRepo::init(root.path().join("dest")).unwrap();
        let state_path = root.path().join("state.db");
        let state = State::open(&state_path).unwrap();

        commit(&source, &[("a.txt", b"a")], "Alice");
        commit(&source, &[("a.txt", b"a"), ("b.txt", b"b")], "Bob");

        let transforms: Vec<Box<dyn Transformation>> = vec![Box::new(Filter::new(["**"]))];
        let transform_refs: Vec<&dyn Transformation> = transforms.iter().map(|b| &**b).collect();

        let first = replay_history(&source, &dest, &transform_refs, Some(&state), None, None).unwrap();
        assert_eq!(first.commits_replayed, 2);

        // A third commit arrives in the source.
        commit(
            &source,
            &[("a.txt", b"a"), ("b.txt", b"b"), ("c.txt", b"c")],
            "Carol",
        );
        let second = replay_history(&source, &dest, &transform_refs, Some(&state), None, None).unwrap();
        assert_eq!(second.commits_replayed, 1);
        assert_eq!(second.commits_skipped, 2);

        // One mapping row per source commit (3 total).
        assert_eq!(state.mappings().unwrap().len(), 3);
        assert_eq!(history_ids(&dest).len(), 3);
    }

    #[test]
    fn replay_applies_author_mapping_and_message_prefix() {
        let root = tempfile::TempDir::new().unwrap();
        let source = GitRepo::init(root.path().join("source")).unwrap();
        let dest = GitRepo::init(root.path().join("dest")).unwrap();

        commit(&source, &[("a.txt", b"a")], "Alice");
        let source_head = source.head_commit_id().unwrap().unwrap();
        let info = source.commit_info(&source_head).unwrap();
        let original_email = info.author.email.clone();

        let transforms: Vec<Box<dyn Transformation>> = vec![Box::new(Filter::new(["**"]))];
        let transform_refs: Vec<&dyn Transformation> = transforms.iter().map(|b| &**b).collect();

        let mut author_map = BTreeMap::new();
        author_map.insert(original_email.clone(), "alice@public.example.com".to_owned());

        let report = replay_history(
            &source,
            &dest,
            &transform_refs,
            None,
            Some(&author_map),
            Some("[mirror] "),
        )
        .unwrap();
        assert_eq!(report.commits_replayed, 1);

        let dest_head = dest.head_commit_id().unwrap().unwrap();
        let dest_info = dest.commit_info(&dest_head).unwrap();
        assert_eq!(dest_info.author.email, "alice@public.example.com");
        assert_eq!(dest_info.committer.email, "alice@public.example.com");
        assert!(
            dest_info.message.starts_with("[mirror] "),
            "expected prefix, got: {}",
            dest_info.message
        );
        assert!(
            dest_info.message.contains(&format!("RepoSync-Generated: {source_head}")),
            "expected generated marker, got: {}",
            dest_info.message
        );
    }
}
