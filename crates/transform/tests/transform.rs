use reposync_core::{Blob, FileEntry, RepoPath, RepositorySnapshot};
use reposync_transform::{Copy, Delete, Filter, Move, Rename, Runner, Transformation};

fn file(path: &str, content: &[u8]) -> FileEntry {
    FileEntry::regular(RepoPath::new(path).unwrap(), Blob::from_bytes(content))
}

fn snapshot() -> RepositorySnapshot {
    let mut snapshot = RepositorySnapshot::new();
    snapshot.insert(file("README.md", b"# readme"));
    snapshot.insert(file("LICENSE", b"MIT"));
    snapshot.insert(file("packages/sdk/lib.rs", b"pub fn f() {}"));
    snapshot.insert(file("packages/sdk/src/mod.rs", b"pub mod f;"));
    snapshot.insert(file("packages/sdk/tests/unit.rs", b"#[test]"));
    snapshot.insert(file("packages/other/tool.rs", b"tool"));
    snapshot.insert(file("scripts/run.sh", b"#!/bin/sh"));
    snapshot.insert(file("docs/guide.md", b"guide"));
    snapshot.insert(file("config/secrets/api.key", b"secret"));
    snapshot
}

fn run(transforms: &[&dyn Transformation], snapshot: RepositorySnapshot) -> RepositorySnapshot {
    Runner::new()
        .run(snapshot, transforms)
        .expect("pipeline runs cleanly")
        .snapshot
}

#[test]
fn filter_keeps_only_matching_globs() {
    let filter = Filter::new(["packages/sdk/**", "LICENSE"]);
    let out = run(&[&filter], snapshot());
    assert_eq!(out.len(), 4);
    assert!(out.contains(&RepoPath::new("packages/sdk/lib.rs").unwrap()));
    assert!(out.contains(&RepoPath::new("packages/sdk/src/mod.rs").unwrap()));
    assert!(out.contains(&RepoPath::new("packages/sdk/tests/unit.rs").unwrap()));
    assert!(out.contains(&RepoPath::new("LICENSE").unwrap()));
    assert!(!out.contains(&RepoPath::new("README.md").unwrap()));
    assert!(!out.contains(&RepoPath::new("config/secrets/api.key").unwrap()));
}

#[test]
fn delete_removes_matching_globs() {
    let delete = Delete::new(["docs/**", "packages/other/**"]);
    let out = run(&[&delete], snapshot());
    assert_eq!(out.len(), 7);
    assert!(!out.contains(&RepoPath::new("docs/guide.md").unwrap()));
    assert!(!out.contains(&RepoPath::new("packages/other/tool.rs").unwrap()));
}

#[test]
fn delete_catches_secrets_anywhere() {
    let delete = Delete::new(["**/secrets/**"]);
    let out = run(&[&delete], snapshot());
    assert!(!out.contains(&RepoPath::new("config/secrets/api.key").unwrap()));
    assert!(out.contains(&RepoPath::new("README.md").unwrap()));
}

#[test]
fn pipeline_matches_ok_yaml_example() {
    let filter = Filter::new(["packages/sdk/**", "LICENSE"]);
    let mv = Move::new("packages/sdk", ".");
    let delete = Delete::new(["**/.internal", "**/secrets/**"]);

    let report = Runner::new().run(snapshot(), &[&filter, &mv, &delete]).unwrap();
    let out = &report.snapshot;
    assert_eq!(out.len(), 4);
    assert!(out.contains(&RepoPath::new("lib.rs").unwrap()));
    assert!(out.contains(&RepoPath::new("src/mod.rs").unwrap()));
    assert!(out.contains(&RepoPath::new("tests/unit.rs").unwrap()));
    assert!(out.contains(&RepoPath::new("LICENSE").unwrap()));
    assert!(!out.contains(&RepoPath::new("packages/sdk/lib.rs").unwrap()));
    assert_eq!(report.steps.len(), 3);
    assert_eq!(report.steps[1].name, "move");
    assert_eq!(report.steps[1].changed, 3);
}

#[test]
fn move_to_subdirectory_relocates_subtree() {
    let mv = Move::new("scripts", "public/scripts");
    let out = run(&[&mv], snapshot());
    assert!(!out.contains(&RepoPath::new("scripts/run.sh").unwrap()));
    assert!(out.contains(&RepoPath::new("public/scripts/run.sh").unwrap()));
    assert_eq!(
        out.get(&RepoPath::new("public/scripts/run.sh").unwrap())
            .unwrap()
            .content
            .content(),
        b"#!/bin/sh"
    );
}

#[test]
fn copy_keeps_originals() {
    let cp = Copy::new("scripts", "backup/scripts");
    let out = run(&[&cp], snapshot());
    assert!(out.contains(&RepoPath::new("scripts/run.sh").unwrap()));
    assert!(out.contains(&RepoPath::new("backup/scripts/run.sh").unwrap()));
    assert_eq!(out.len(), 10);
}

#[test]
fn rename_moves_a_single_file() {
    let rename = Rename::new("README.md", "docs/README.md");
    let out = run(&[&rename], snapshot());
    assert!(!out.contains(&RepoPath::new("README.md").unwrap()));
    assert!(out.contains(&RepoPath::new("docs/README.md").unwrap()));
    assert_eq!(
        out.get(&RepoPath::new("docs/README.md").unwrap())
            .unwrap()
            .content
            .content(),
        b"# readme"
    );
}

#[test]
fn move_detects_collision_with_existing_files() {
    let mut snapshot = RepositorySnapshot::new();
    snapshot.insert(file("a/one.txt", b"1"));
    snapshot.insert(file("b/one.txt", b"existing"));

    let mv = Move::new("a", "b");
    let result = Runner::new().run(snapshot, &[&mv]);
    assert!(result.is_err());
}

#[test]
fn rename_detects_collision_with_existing_files() {
    let rename = Rename::new("LICENSE", "README.md");
    let result = Runner::new().run(snapshot(), &[&rename]);
    assert!(result.is_err());
}

#[test]
fn move_matching_nothing_warns_but_succeeds() {
    let mv = Move::new("missing/dir", "target");
    let report = Runner::new().run(snapshot(), &[&mv]).unwrap();
    assert_eq!(report.snapshot.len(), 9);
    assert_eq!(report.steps[0].changed, 0);
    assert_eq!(report.steps[0].warnings.len(), 1);
}

#[test]
fn runner_stops_on_error() {
    let delete = Delete::new(["c.txt"]);
    let rename = Rename::new("a/one.txt", "b/one.txt");
    let result = Runner::new().run(snapshot(), &[&delete, &rename]);
    assert!(result.is_err());
}

#[test]
fn runner_reports_steps_and_timing() {
    let delete = Delete::new(["docs/**"]);
    let report = Runner::new().run(snapshot(), &[&delete]).unwrap();
    assert_eq!(report.steps.len(), 1);
    assert_eq!(report.steps[0].name, "delete");
    assert_eq!(report.steps[0].changed, 1);
    assert!(report.steps[0].duration.as_nanos() > 0);
    assert_eq!(report.events.len(), 1);
    assert_eq!(report.warnings.len(), 0);
}

#[test]
fn runs_are_deterministic() {
    let filter = Filter::new(["packages/sdk/**", "LICENSE"]);
    let mv = Move::new("packages/sdk", ".");
    let delete = Delete::new(["**/.internal", "**/secrets/**"]);

    let first = Runner::new().run(snapshot(), &[&filter, &mv, &delete]).unwrap();
    let second = Runner::new().run(snapshot(), &[&filter, &mv, &delete]).unwrap();

    assert_eq!(first.snapshot, second.snapshot);
    assert_eq!(first.events, second.events);
    assert_eq!(first.steps.len(), second.steps.len());
}
