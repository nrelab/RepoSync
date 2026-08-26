//! End-to-end test of bidirectional sync loop prevention (`reposync sync`).

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
        message: format!("by {author}"),
        author: Some(signature.clone()),
        committer: Some(signature),
    };
    repo.write_commit(&snapshot, &spec).unwrap();
}

#[test]
fn sync_twice_recognizes_generated_and_avoids_loops() {
    let root = tempfile::TempDir::new().unwrap();
    let base = root.path();
    let internal = base.join("internal.git");
    let public = base.join("public.git");
    let state_path = base.join(".sync").join("state.db");
    let config_path = base.join("config.yaml");

    // Internal source of truth with two commits.
    let internal_repo = GitRepo::init_bare(&internal).unwrap();
    commit(&internal_repo, &[("a.txt", b"a")], "Alice");
    commit(
        &internal_repo,
        &[("a.txt", b"a"), ("b.txt", b"b")],
        "Bob",
    );

    // Fresh public mirror.
    GitRepo::init_bare(&public).unwrap();

    std::fs::write(
        &config_path,
        format!(
            "pipeline:\n  name: sync-test\nsource:\n  type: git\n  url: {}\n  ref: main\ntransform:\n  - filter:\n      paths:\n        - \"**\"\ndestination:\n  type: git\n  url: {}\n  branch: main\n",
            internal.display(),
            public.display(),
        ),
    )
    .unwrap();

    // First sync: forward-replays 2 internal commits to public.
    let first = Command::new(BIN)
        .arg("sync")
        .arg(&config_path)
        .arg("--state")
        .arg(&state_path)
        .output()
        .unwrap();
    assert!(
        first.status.success(),
        "first sync failed:\n{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let first_out = String::from_utf8_lossy(&first.stdout);
    assert!(
        first_out.contains("2 forward"),
        "expected 2 forward replays, got:\n{first_out}"
    );

    // Second sync: everything already generated/mapped -> no-op, no loop.
    let second = Command::new(BIN)
        .arg("sync")
        .arg(&config_path)
        .arg("--state")
        .arg(&state_path)
        .output()
        .unwrap();
    assert!(
        second.status.success(),
        "second sync failed:\n{}",
        String::from_utf8_lossy(&second.stderr)
    );
    let second_out = String::from_utf8_lossy(&second.stdout);
    assert!(
        second_out.contains("0 forward"),
        "expected no forward replays on second run, got:\n{second_out}"
    );

    // Exactly two mapping rows; public and internal each hold exactly two
    // commits (no duplicate/looped history).
    let state = State::open(&state_path).unwrap();
    assert_eq!(state.mappings().unwrap().len(), 2);

    let public_repo = GitRepo::open(&public).unwrap();
    assert_eq!(public_repo.history(None).unwrap().len(), 2);
    let internal_repo2 = GitRepo::open(&internal).unwrap();
    assert_eq!(internal_repo2.history(None).unwrap().len(), 2);
}
