use std::fs;
use std::path::Path;

use git2::{Commit, IndexAddOption, Repository};
use reposync_core::{Blob, FileEntry, FileMode, RepoPath, RepositorySnapshot};
use reposync_git::{CommitSpec, GitRepo};
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

#[test]
fn fresh_repo_has_no_head() {
    let dir = TempDir::new().unwrap();
    let repo_path = dir.path().join("repo");
    init_repo(&repo_path);

    let repo = GitRepo::open(&repo_path).unwrap();
    assert_eq!(repo.head_commit_id().unwrap(), None);
    assert!(repo.head_snapshot().unwrap().is_empty());
    assert!(repo.history(None).unwrap().is_empty());
}

#[test]
fn clone_reads_snapshot_of_working_tree() {
    let source = TempDir::new().unwrap();
    let repo = init_repo(source.path());
    fs::write(source.path().join("README.md"), "# hello").unwrap();
    fs::create_dir_all(source.path().join("src")).unwrap();
    fs::write(source.path().join("src/lib.rs"), "pub fn f() {}").unwrap();
    commit_worktree(&repo, "initial commit");

    let dest = TempDir::new().unwrap();
    let cloned =
        GitRepo::clone(source.path().to_str().unwrap(), dest.path().join("repo")).unwrap();

    let snapshot = cloned.head_snapshot().unwrap();
    assert_eq!(snapshot.len(), 2);
    let lib = snapshot.get(&RepoPath::new("src/lib.rs").unwrap()).unwrap();
    assert_eq!(lib.content.content(), b"pub fn f() {}");
    assert!(snapshot.contains(&RepoPath::new("README.md").unwrap()));
    assert!(snapshot.metadata().head.is_some());
}

#[test]
fn write_commit_round_trips_content_and_modes() {
    let dir = TempDir::new().unwrap();
    let repo_path = dir.path().join("repo");
    init_repo(&repo_path);
    let repo = GitRepo::open(&repo_path).unwrap();

    let mut snapshot = RepositorySnapshot::new();
    snapshot.insert(FileEntry::regular(
        RepoPath::new("target.txt").unwrap(),
        Blob::from_bytes(b"payload"),
    ));
    snapshot.insert(FileEntry::new(
        RepoPath::new("script.sh").unwrap(),
        Blob::from_bytes(b"#!/bin/sh\necho hi\n"),
        FileMode::Executable,
    ));
    snapshot.insert(FileEntry::new(
        RepoPath::new("link").unwrap(),
        Blob::from_bytes(b"target.txt"),
        FileMode::Symlink,
    ));
    snapshot.insert(FileEntry::regular(
        RepoPath::new("nested/deep/file.txt").unwrap(),
        Blob::from_bytes(b"nested"),
    ));

    let commit_id = repo
        .write_commit(&snapshot, &CommitSpec::new("import snapshot"))
        .unwrap();

    let read_back = repo.head_snapshot().unwrap();
    assert_eq!(read_back.files, snapshot.files);
    assert_eq!(read_back.metadata().head.as_ref(), Some(&commit_id));
    assert_eq!(
        read_back.metadata().head_message.as_deref(),
        Some("import snapshot")
    );
}

#[test]
fn commits_form_a_chain() {
    let dir = TempDir::new().unwrap();
    let repo_path = dir.path().join("repo");
    init_repo(&repo_path);
    let repo = GitRepo::open(&repo_path).unwrap();

    let mut first = RepositorySnapshot::new();
    first.insert(FileEntry::regular(
        RepoPath::new("a.txt").unwrap(),
        Blob::from_bytes(b"a"),
    ));
    repo.write_commit(&first, &CommitSpec::new("first")).unwrap();

    let mut second = first.clone();
    second.insert(FileEntry::regular(
        RepoPath::new("b.txt").unwrap(),
        Blob::from_bytes(b"b"),
    ));
    repo.write_commit(&second, &CommitSpec::new("second")).unwrap();

    let history = repo.history(None).unwrap();
    assert_eq!(history.len(), 2);
    assert_ne!(history[0], history[1]);
    assert_eq!(history[0], repo.head_commit_id().unwrap().unwrap());

    assert_eq!(repo.head_snapshot().unwrap().len(), 2);
}

#[test]
fn checkout_writes_snapshot_to_worktree() {
    let dir = TempDir::new().unwrap();
    let repo_path = dir.path().join("repo");
    let repo = init_repo(&repo_path);
    fs::write(repo_path.join("base.txt"), "base").unwrap();
    commit_worktree(&repo, "base");
    drop(repo);

    let gitrepo = GitRepo::open(&repo_path).unwrap();
    let mut snapshot = gitrepo.head_snapshot().unwrap();
    snapshot.insert(FileEntry::regular(
        RepoPath::new("new.txt").unwrap(),
        Blob::from_bytes(b"fresh"),
    ));
    gitrepo
        .write_commit(&snapshot, &CommitSpec::new("add new"))
        .unwrap();
    gitrepo.checkout().unwrap();

    assert_eq!(fs::read(repo_path.join("new.txt")).unwrap(), b"fresh");
    assert!(gitrepo.status_is_clean().unwrap());
}

#[test]
fn push_and_fetch_share_commits() {
    let dir = TempDir::new().unwrap();
    let origin = dir.path().join("origin.git");
    Repository::init_bare(&origin).unwrap();

    let a_path = dir.path().join("a");
    let a = GitRepo::clone(origin.to_str().unwrap(), &a_path).unwrap();
    a.set_identity("Alice", "alice@example.com").unwrap();
    assert_eq!(a.head_commit_id().unwrap(), None);

    let mut snapshot = RepositorySnapshot::new();
    snapshot.insert(FileEntry::regular(
        RepoPath::new("hello.txt").unwrap(),
        Blob::from_bytes(b"world"),
    ));
    a.write_commit(&snapshot, &CommitSpec::new("add hello"))
        .unwrap();
    a.push(origin.to_str().unwrap()).unwrap();

    let b = GitRepo::clone(origin.to_str().unwrap(), dir.path().join("b")).unwrap();
    let b_snapshot = b.head_snapshot().unwrap();
    assert!(b_snapshot.contains(&RepoPath::new("hello.txt").unwrap()));
    assert_eq!(
        b_snapshot.metadata().head,
        a.head_commit_id().unwrap()
    );

    let branch = a.current_branch_name().unwrap();
    let mut second = snapshot.clone();
    second.insert(FileEntry::regular(
        RepoPath::new("second.txt").unwrap(),
        Blob::from_bytes(b"two"),
    ));
    a.write_commit(&second, &CommitSpec::new("add second"))
        .unwrap();
    a.push(origin.to_str().unwrap()).unwrap();

    b.fetch(
        origin.to_str().unwrap(),
        "refs/heads/*:refs/remotes/origin/*",
    )
    .unwrap();
    let remote_ref = format!("refs/remotes/origin/{branch}");
    let remote_snapshot = b.snapshot_at_ref(&remote_ref).unwrap();
    assert!(remote_snapshot.contains(&RepoPath::new("second.txt").unwrap()));
}
