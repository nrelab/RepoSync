//! End-to-end test of history-mode migration (`reposync migrate --history`).

use std::process::Command;

use reposync_core::{Blob, FileEntry, RepoPath, RepositorySnapshot, Signature};
use reposync_git::{CommitSpec, GitRepo};
use reposync_state::State;

const BIN: &str = env!("CARGO_BIN_EXE_reposync");

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

#[test]
fn migrate_history_replays_all_commits_and_is_idempotent() {
    let root = tempfile::TempDir::new().unwrap();
    let base = root.path();
    let source = base.join("source");
    let dest = base.join("dest.git");
    let state_path = base.join(".sync").join("state.db");
    let config_path = base.join("config.yaml");

    // A 4-commit source history, each commit accumulating a file.
    let source_repo = GitRepo::init(&source).unwrap();
    commit(&source_repo, &[("a.txt", b"a")], "Alice");
    commit(&source_repo, &[("a.txt", b"a"), ("b.txt", b"b")], "Bob");
    commit(
        &source_repo,
        &[("a.txt", b"a"), ("b.txt", b"b"), ("c.txt", b"c")],
        "Carol",
    );
    commit(
        &source_repo,
        &[("a.txt", b"a"), ("b.txt", b"b"), ("c.txt", b"c"), ("d.txt", b"d")],
        "Dan",
    );

    // Fresh bare destination (no initial commit) so history replay owns it.
    GitRepo::init_bare(&dest).unwrap();

    std::fs::write(
        &config_path,
        format!(
            "pipeline:\n  name: history-test\nsource:\n  type: git\n  url: {}\n  ref: main\ntransform:\n  - filter:\n      paths:\n        - \"**\"\ndestination:\n  type: git\n  url: {}\n  branch: main\n",
            source.display(),
            dest.display(),
        ),
    )
    .unwrap();

    // First run: replay 4 commits.
    let first = Command::new(BIN)
        .arg("migrate")
        .arg(&config_path)
        .arg("--state")
        .arg(&state_path)
        .arg("--history")
        .output()
        .unwrap();
    assert!(
        first.status.success(),
        "first history migrate failed:\n{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let first_out = String::from_utf8_lossy(&first.stdout);
    assert!(
        first_out.contains("History migrated: 4 commit(s) replayed"),
        "unexpected first-run output:\n{first_out}"
    );

    // Second run: everything already mapped -> no-op.
    let second = Command::new(BIN)
        .arg("migrate")
        .arg(&config_path)
        .arg("--state")
        .arg(&state_path)
        .arg("--history")
        .output()
        .unwrap();
    assert!(
        second.status.success(),
        "second history migrate failed:\n{}",
        String::from_utf8_lossy(&second.stderr)
    );
    let second_out = String::from_utf8_lossy(&second.stdout);
    assert!(
        second_out.contains("Already synced: 4 source commit(s) already mapped"),
        "expected idempotent second run, got:\n{second_out}"
    );

    // Exactly four mapping rows.
    let state = State::open(&state_path).unwrap();
    assert_eq!(state.mappings().unwrap().len(), 4);

    // Destination has 4 commits with a correct parent chain and authors.
    let dest_repo = GitRepo::open(&dest).unwrap();
    let ids = dest_repo.history(None).unwrap().into_iter().rev().collect::<Vec<_>>();
    assert_eq!(ids.len(), 4, "expected 4 commits in destination");
    for window in ids.windows(2) {
        let child = dest_repo.commit_info(&window[1]).unwrap();
        assert_eq!(child.parents, vec![window[0].clone()]);
    }
    let authors: Vec<String> = ids
        .iter()
        .map(|id| dest_repo.commit_info(id).unwrap().author.name)
        .collect();
    assert_eq!(authors, vec!["Alice", "Bob", "Carol", "Dan"]);
}
