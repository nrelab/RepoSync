//! End-to-end test of the incremental migrate guard (M10).
//!
//! Running `reposync migrate` twice with the same state database must:
//!   * make the second run a no-op (skip already-mapped source commits), and
//!   * record exactly one source→destination mapping row.

use std::process::Command;

use reposync_core::{Blob, FileEntry, RepoPath, RepositorySnapshot};
use reposync_git::{CommitSpec, GitRepo};
use reposync_state::State;

const BIN: &str = env!("CARGO_BIN_EXE_reposync");

fn seed(repo: &GitRepo, path: &str, content: &[u8]) {
    repo.set_identity("Test", "test@example.com").unwrap();
    let mut snapshot = RepositorySnapshot::new();
    snapshot.insert(FileEntry::regular(
        RepoPath::new(path).unwrap(),
        Blob::from_bytes(content),
    ));
    repo.write_commit(&snapshot, &CommitSpec::new("seed")).unwrap();
}

#[test]
fn migrate_twice_is_idempotent_and_records_one_mapping() {
    let root = tempfile::TempDir::new().unwrap();
    let base = root.path();
    let source = base.join("source");
    let dest = base.join("dest.git");
    let state_path = base.join(".sync").join("state.db");
    let config_path = base.join("config.yaml");

    let source_repo = GitRepo::init(&source).unwrap();
    seed(&source_repo, "lib.rs", b"pub fn f() {}");

    let dest_repo = GitRepo::init_bare(&dest).unwrap();
    seed(&dest_repo, "README.md", b"destination");

    std::fs::write(
        &config_path,
        format!(
            "pipeline:\n  name: incremental-test\nsource:\n  type: git\n  url: {}\n  ref: main\ntransform:\n  - filter:\n      paths:\n        - \"**\"\ndestination:\n  type: git\n  url: {}\n  branch: main\n",
            source.display(),
            dest.display(),
        ),
    )
    .unwrap();

    // First run: should create a commit and record a mapping.
    let first = Command::new(BIN)
        .arg("migrate")
        .arg(&config_path)
        .arg("--state")
        .arg(&state_path)
        .output()
        .unwrap();
    assert!(
        first.status.success(),
        "first migrate failed:\n{}",
        String::from_utf8_lossy(&first.stderr)
    );

    // Second run: source commit already mapped -> no-op.
    let second = Command::new(BIN)
        .arg("migrate")
        .arg(&config_path)
        .arg("--state")
        .arg(&state_path)
        .output()
        .unwrap();
    assert!(
        second.status.success(),
        "second migrate failed:\n{}",
        String::from_utf8_lossy(&second.stderr)
    );
    let second_out = String::from_utf8_lossy(&second.stdout);
    assert!(
        second_out.contains("Already synced"),
        "expected second run to be a no-op, got:\n{second_out}"
    );

    // Exactly one mapping row.
    let state = State::open(&state_path).unwrap();
    let mappings = state.mappings().unwrap();
    assert_eq!(
        mappings.len(),
        1,
        "expected exactly one mapping row, got {}",
        mappings.len()
    );

    // Destination has exactly the initial seed commit plus the migrated commit.
    let dest_repo = GitRepo::open(&dest).unwrap();
    let history = dest_repo.history(None).unwrap();
    assert_eq!(
        history.len(),
        2,
        "destination should have exactly 2 commits (seed + migrated), got {}",
        history.len()
    );
}
