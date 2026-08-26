use std::fs;
use std::path::Path;

use git2::{Commit, IndexAddOption, Repository};
use reposync_core::{Blob, FileEntry, RepoPath};
use reposync_git::{CommitSpec, GitRepo};
use reposync_sync::Syncer;
use reposync_transform::{Filter, Move, Rename};
use tempfile::TempDir;

fn init_repo(dir: &Path) -> Repository {
    let repo = Repository::init(dir).unwrap();
    let mut config = repo.config().unwrap();
    config.set_str("user.name", "Test User").unwrap();
    config.set_str("user.email", "test@example.com").unwrap();
    repo
}

fn commit_worktree(repo: &Repository, message: &str) {
    let mut index = repo.index().unwrap();
    index
        .add_all(["*"], IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let signature = repo.signature().unwrap();
    let parents: Vec<Commit> = match repo.head() {
        Ok(head) => vec![head.peel_to_commit().unwrap()],
        Err(_) => vec![],
    };
    let parent_refs: Vec<&Commit> = parents.iter().collect();
    repo.commit(Some("HEAD"), &signature, &signature, message, &tree, &parent_refs)
        .unwrap();
}

fn write_source_files(dir: &Path) {
    fs::write(dir.join("README.md"), "# readme").unwrap();
    fs::write(dir.join("LICENSE"), "MIT").unwrap();
    fs::create_dir_all(dir.join("packages/sdk/src")).unwrap();
    fs::create_dir_all(dir.join("packages/sdk/tests")).unwrap();
    fs::write(dir.join("packages/sdk/lib.rs"), "pub fn f() {}").unwrap();
    fs::write(dir.join("packages/sdk/src/mod.rs"), "pub mod f;").unwrap();
    fs::write(dir.join("packages/sdk/tests/unit.rs"), "#[test]").unwrap();
    fs::create_dir_all(dir.join("packages/other")).unwrap();
    fs::write(dir.join("packages/other/tool.rs"), "tool").unwrap();
    fs::create_dir_all(dir.join("scripts")).unwrap();
    fs::write(dir.join("scripts/run.sh"), "#!/bin/sh").unwrap();
    fs::create_dir_all(dir.join("docs")).unwrap();
    fs::write(dir.join("docs/guide.md"), "guide").unwrap();
    fs::create_dir_all(dir.join("config/secrets")).unwrap();
    fs::write(dir.join("config/secrets/api.key"), "secret").unwrap();
}

fn transforms() -> (Filter, Move) {
    (
        Filter::new(["packages/sdk/**", "LICENSE"]),
        Move::new("packages/sdk", "."),
    )
}

/// Build a source repo with content and a fresh destination clone of a bare
/// origin.
fn setup(dir: &TempDir) -> (GitRepo, GitRepo, std::path::PathBuf) {
    let source_path = dir.path().join("source");
    let source_repo = init_repo(&source_path);
    write_source_files(&source_path);
    commit_worktree(&source_repo, "initial");
    drop(source_repo);
    let source = GitRepo::open(&source_path).unwrap();

    let origin = dir.path().join("origin.git");
    Repository::init_bare(&origin).unwrap();

    let dest_path = dir.path().join("dest");
    let dest = GitRepo::clone(origin.to_str().unwrap(), &dest_path).unwrap();
    dest.set_identity("Bob", "bob@example.com").unwrap();

    (source, dest, origin)
}

#[test]
fn end_to_end_source_to_destination() {
    let dir = TempDir::new().unwrap();
    let (source, dest, origin) = setup(&dir);
    let (filter, mv) = transforms();
    let syncer = Syncer::new(&source, &dest);

    let report = syncer
        .run(
            &[&filter, &mv],
            &CommitSpec::new("sync public sdk"),
            Some(origin.to_str().unwrap()),
        )
        .unwrap();

    assert!(report.changed);
    assert!(report.commit_id.is_some());
    assert!(report.pushed);
    assert_eq!(report.diff.added.len(), 4);
    assert_eq!(report.steps.len(), 2);
    assert_eq!(report.steps[1].name, "move");
    assert_eq!(report.steps[1].changed, 3);

    let verify = GitRepo::clone(origin.to_str().unwrap(), dir.path().join("verify")).unwrap();
    let snapshot = verify.head_snapshot().unwrap();
    assert_eq!(snapshot.len(), 4);
    assert!(snapshot.contains(&RepoPath::new("lib.rs").unwrap()));
    assert!(snapshot.contains(&RepoPath::new("src/mod.rs").unwrap()));
    assert!(snapshot.contains(&RepoPath::new("tests/unit.rs").unwrap()));
    assert!(snapshot.contains(&RepoPath::new("LICENSE").unwrap()));
    assert!(!snapshot.contains(&RepoPath::new("README.md").unwrap()));
    assert!(!snapshot.contains(&RepoPath::new("config/secrets/api.key").unwrap()));
}

#[test]
fn no_op_run_makes_zero_commits() {
    let dir = TempDir::new().unwrap();
    let (source, dest, origin) = setup(&dir);
    let (filter, mv) = transforms();
    let syncer = Syncer::new(&source, &dest);
    let commit = CommitSpec::new("sync public sdk");

    let first = syncer.run(&[&filter, &mv], &commit, Some(origin.to_str().unwrap())).unwrap();
    assert!(first.changed);

    let second = syncer.run(&[&filter, &mv], &commit, Some(origin.to_str().unwrap())).unwrap();
    assert!(!second.changed);
    assert_eq!(second.commit_id, None);
    assert!(!second.pushed);
    assert!(second.diff.is_empty());

    assert_eq!(dest.history(None).unwrap().len(), 1);
}

#[test]
fn source_change_triggers_incremental_sync() {
    let dir = TempDir::new().unwrap();
    let (source, dest, origin) = setup(&dir);
    let (filter, mv) = transforms();
    let syncer = Syncer::new(&source, &dest);

    syncer
        .run(&[&filter, &mv], &CommitSpec::new("first"), Some(origin.to_str().unwrap()))
        .unwrap();

    let mut updated = source.head_snapshot().unwrap();
    updated.insert(FileEntry::regular(
        RepoPath::new("packages/sdk/extra.rs").unwrap(),
        Blob::from_bytes(b"extra"),
    ));
    source
        .write_commit(&updated, &CommitSpec::new("add extra to source"))
        .unwrap();

    let report = syncer
        .run(&[&filter, &mv], &CommitSpec::new("sync extra"), Some(origin.to_str().unwrap()))
        .unwrap();
    assert!(report.changed);
    assert_eq!(report.diff.added.len(), 1);
    assert_eq!(
        report.commit_id.as_ref(),
        Some(&dest.head_commit_id().unwrap().unwrap())
    );

    let verify = GitRepo::clone(origin.to_str().unwrap(), dir.path().join("verify")).unwrap();
    let snapshot = verify.head_snapshot().unwrap();
    assert!(snapshot.contains(&RepoPath::new("extra.rs").unwrap()));
    assert_eq!(snapshot.len(), 5);
}

#[test]
fn failed_transform_leaves_destination_untouched() {
    let dir = TempDir::new().unwrap();
    let (source, dest, origin) = setup(&dir);
    let (filter, mv) = transforms();
    let collision = Rename::new("lib.rs", "LICENSE");
    let syncer = Syncer::new(&source, &dest);

    let result = syncer.run(
        &[&filter, &mv, &collision],
        &CommitSpec::new("boom"),
        Some(origin.to_str().unwrap()),
    );
    assert!(result.is_err());

    assert_eq!(dest.head_commit_id().unwrap(), None);
    let bare = Repository::open_bare(&origin).unwrap();
    assert!(bare.find_branch("master", git2::BranchType::Local).is_err());
}
