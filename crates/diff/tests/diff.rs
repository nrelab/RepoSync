use reposync_core::{Blob, FileEntry, FileMode, RepoPath, RepositorySnapshot};
use reposync_diff::{diff, SnapshotDiff};
use reposync_transform::{Filter, Move, Runner};

fn file(path: &str, content: &[u8]) -> FileEntry {
    FileEntry::regular(RepoPath::new(path).unwrap(), Blob::from_bytes(content))
}

fn snapshot(files: &[(&str, &[u8])]) -> RepositorySnapshot {
    let mut snapshot = RepositorySnapshot::new();
    for (path, content) in files {
        snapshot.insert(file(path, content));
    }
    snapshot
}

fn input_snapshot() -> RepositorySnapshot {
    snapshot(&[
        ("README.md", b"# readme"),
        ("LICENSE", b"MIT"),
        ("packages/sdk/lib.rs", b"pub fn f() {}"),
        ("packages/sdk/src/mod.rs", b"pub mod f;"),
        ("packages/sdk/tests/unit.rs", b"#[test]"),
        ("packages/other/tool.rs", b"tool"),
        ("scripts/run.sh", b"#!/bin/sh"),
        ("docs/guide.md", b"guide"),
        ("config/secrets/api.key", b"secret"),
    ])
}

#[test]
fn identical_snapshots_diff_is_empty() {
    let snapshot = snapshot(&[("a.txt", b"a"), ("b/c.txt", b"c")]);
    let result = diff(&snapshot, &snapshot);
    assert!(result.is_empty());
    assert_eq!(result.changes(), 0);
}

#[test]
fn added_removed_and_modified() {
    let old = snapshot(&[("a.txt", b"a"), ("gone.txt", b"g")]);
    let new = snapshot(&[("a.txt", b"changed"), ("added.txt", b"n")]);
    let result = diff(&old, &new);

    assert_eq!(result.added.len(), 1);
    assert_eq!(result.added[0].path.as_str(), "added.txt");
    assert_eq!(result.added[0].old_hash, None);
    assert!(result.added[0].new_hash.is_some());

    assert_eq!(result.removed.len(), 1);
    assert_eq!(result.removed[0].path.as_str(), "gone.txt");
    assert_eq!(result.removed[0].new_hash, None);

    assert_eq!(result.modified.len(), 1);
    assert_eq!(result.modified[0].path.as_str(), "a.txt");
    assert_ne!(result.modified[0].old_hash, result.modified[0].new_hash);
    assert_eq!(result.modified[0].old_mode, result.modified[0].new_mode);

    assert!(result.renamed.is_empty());
}

#[test]
fn mode_only_change_is_modified_not_renamed() {
    let mut old = RepositorySnapshot::new();
    old.insert(FileEntry::new(
        RepoPath::new("run.sh").unwrap(),
        Blob::from_bytes(b"#!/bin/sh"),
        FileMode::File,
    ));
    let mut new = old.clone();
    new.get_mut(&RepoPath::new("run.sh").unwrap())
        .unwrap()
        .mode = FileMode::Executable;

    let result = diff(&old, &new);
    assert_eq!(result.modified.len(), 1);
    assert_eq!(result.modified[0].old_mode, Some(FileMode::File));
    assert_eq!(result.modified[0].new_mode, Some(FileMode::Executable));
    assert!(result.renamed.is_empty());
}

#[test]
fn rename_detects_identical_content_and_mode() {
    let old = snapshot(&[("src/old.rs", b"code"), ("keep.txt", b"k")]);
    let new = snapshot(&[("src/new.rs", b"code"), ("keep.txt", b"k")]);
    let result = diff(&old, &new);

    assert_eq!(result.renamed.len(), 1);
    assert_eq!(result.renamed[0].from.as_str(), "src/old.rs");
    assert_eq!(result.renamed[0].to.as_str(), "src/new.rs");
    assert_eq!(result.added.len(), 0);
    assert_eq!(result.removed.len(), 0);
    assert_eq!(result.modified.len(), 0);
}

#[test]
fn changed_content_is_not_renamed() {
    let old = snapshot(&[("old.txt", b"aaa")]);
    let new = snapshot(&[("new.txt", b"bbb")]);
    let result = diff(&old, &new);
    assert!(result.renamed.is_empty());
    assert_eq!(result.added.len(), 1);
    assert_eq!(result.removed.len(), 1);
}

#[test]
fn copy_is_added_not_renamed() {
    let old = snapshot(&[("a.txt", b"x")]);
    let new = snapshot(&[("a.txt", b"x"), ("copy/a.txt", b"x")]);
    let result = diff(&old, &new);
    assert_eq!(result.added.len(), 1);
    assert!(result.renamed.is_empty());
}

#[test]
fn rename_shuffle_is_deterministic() {
    let old = snapshot(&[("a1.txt", b"x"), ("a2.txt", b"x"), ("b.txt", b"y")]);
    let new = snapshot(&[("b.txt", b"y"), ("z1.txt", b"x"), ("z2.txt", b"x")]);
    let result = diff(&old, &new);

    assert_eq!(result.renamed.len(), 2);
    assert_eq!(result.renamed[0].from.as_str(), "a1.txt");
    assert_eq!(result.renamed[0].to.as_str(), "z1.txt");
    assert_eq!(result.renamed[1].from.as_str(), "a2.txt");
    assert_eq!(result.renamed[1].to.as_str(), "z2.txt");
}

#[test]
fn pipeline_produces_expected_diff() {
    let filter = Filter::new(["packages/sdk/**", "LICENSE"]);
    let mv = Move::new("packages/sdk", ".");
    let input = input_snapshot();
    let report = Runner::new().run(input.clone(), &[&filter, &mv]).unwrap();

    let result: SnapshotDiff = diff(&input, &report.snapshot);

    assert_eq!(result.renamed.len(), 3);
    let renamed: Vec<(String, String)> = result
        .renamed
        .iter()
        .map(|entry| (entry.from.as_str().to_owned(), entry.to.as_str().to_owned()))
        .collect();
    assert!(renamed.contains(&("packages/sdk/lib.rs".into(), "lib.rs".into())));
    assert!(renamed.contains(&("packages/sdk/src/mod.rs".into(), "src/mod.rs".into())));
    assert!(renamed.contains(&("packages/sdk/tests/unit.rs".into(), "tests/unit.rs".into())));

    assert_eq!(result.removed.len(), 5);
    let removed: Vec<&str> = result.removed.iter().map(|entry| entry.path.as_str()).collect();
    for path in [
        "README.md",
        "packages/other/tool.rs",
        "scripts/run.sh",
        "docs/guide.md",
        "config/secrets/api.key",
    ] {
        assert!(removed.contains(&path), "expected `{path}` removed");
    }

    assert!(result.added.is_empty());
    assert!(result.modified.is_empty());
    assert_eq!(result.changes(), 8);
}
