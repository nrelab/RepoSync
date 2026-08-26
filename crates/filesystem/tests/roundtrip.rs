use std::fs;
use std::path::Path;

use reposync_core::{FileMode, RepositorySnapshot};
use reposync_filesystem::{ingest, materialize};

fn write_tree(dir: &Path) {
    fs::create_dir_all(dir.join("src/deep")).unwrap();
    fs::create_dir_all(dir.join("bin")).unwrap();
    fs::write(dir.join("README.md"), b"# hello\n").unwrap();
    fs::write(dir.join("src/main.rs"), b"fn main() {}\n").unwrap();
    fs::write(dir.join("src/deep/util.rs"), b"pub fn u() {}\n").unwrap();
    fs::write(dir.join("bin/tool.sh"), "#!/bin/sh\n").unwrap();
    set_executable(&dir.join("bin/tool.sh"));
    make_symlink("README.md", &dir.join("link"));
}

#[cfg(unix)]
fn set_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) {}

#[cfg(unix)]
fn make_symlink(target: &str, link: &Path) {
    std::os::unix::fs::symlink(target, link).unwrap();
}

#[cfg(not(unix))]
fn make_symlink(_target: &str, _link: &Path) {}

#[test]
fn round_trip_preserves_tree() {
    let source = tempfile::tempdir().unwrap();
    write_tree(source.path());

    let first = ingest(source.path()).unwrap();
    assert_eq!(first.len(), 5);

    let materialized = tempfile::tempdir().unwrap();
    materialize(&first, materialized.path()).unwrap();

    let second = ingest(materialized.path()).unwrap();
    assert_eq!(second, first);
}

#[test]
fn round_trip_preserves_modes() {
    let source = tempfile::tempdir().unwrap();
    write_tree(source.path());

    let snapshot = ingest(source.path()).unwrap();

    let bin = reposync_core::RepoPath::new("bin/tool.sh").unwrap();
    assert_eq!(snapshot.get(&bin).unwrap().mode, FileMode::Executable);

    let materialized = tempfile::tempdir().unwrap();
    materialize(&snapshot, materialized.path()).unwrap();
    let again = ingest(materialized.path()).unwrap();
    assert_eq!(again.get(&bin).unwrap().mode, FileMode::Executable);
}

#[cfg(unix)]
#[test]
fn round_trip_preserves_symlinks() {
    use reposync_core::RepoPath;

    let source = tempfile::tempdir().unwrap();
    write_tree(source.path());

    let snapshot = ingest(source.path()).unwrap();
    let link = RepoPath::new("link").unwrap();
    let entry = snapshot.get(&link).unwrap();
    assert_eq!(entry.mode, FileMode::Symlink);
    assert_eq!(entry.content.content(), b"README.md");

    let materialized = tempfile::tempdir().unwrap();
    materialize(&snapshot, materialized.path()).unwrap();

    let again = ingest(materialized.path()).unwrap();
    assert_eq!(again.get(&link).unwrap().content.content(), b"README.md");
    assert!(materialized.path().join("link").is_symlink());
}

#[test]
fn materialize_clears_preexisting_contents() {
    let source = tempfile::tempdir().unwrap();
    write_tree(source.path());
    let snapshot: RepositorySnapshot = ingest(source.path()).unwrap();

    let target = tempfile::tempdir().unwrap();
    fs::write(target.path().join("stray.txt"), b"remove me").unwrap();
    fs::create_dir_all(target.path().join("obsolete")).unwrap();
    fs::write(target.path().join("obsolete/keep-out.txt"), b"nope").unwrap();

    materialize(&snapshot, target.path()).unwrap();

    assert!(!target.path().join("stray.txt").exists());
    assert!(!target.path().join("obsolete").exists());
    assert!(target.path().join("README.md").exists());
    assert_eq!(ingest(target.path()).unwrap(), snapshot);
}

#[test]
fn ingest_twice_is_deterministic() {
    let dir = tempfile::tempdir().unwrap();
    write_tree(dir.path());

    let first = ingest(dir.path()).unwrap();
    let second = ingest(dir.path()).unwrap();
    assert_eq!(first, second);

    let paths: Vec<&str> = first.paths().map(reposync_core::RepoPath::as_str).collect();
    let mut sorted = paths.clone();
    sorted.sort_unstable();
    assert_eq!(paths, sorted);
}

#[test]
fn ingest_rejects_non_directory() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("not-a-dir.txt");
    fs::write(&file, b"x").unwrap();
    assert!(ingest(&file).is_err());
}

#[test]
fn materialize_rejects_gitlink() {
    use reposync_core::{Blob, FileEntry, FileMode, RepoPath};

    let mut snapshot = RepositorySnapshot::new();
    snapshot.insert(FileEntry::new(
        RepoPath::new("sub").unwrap(),
        Blob::new(b"abc123".to_vec()),
        FileMode::Gitlink,
    ));

    let target = tempfile::tempdir().unwrap();
    assert!(materialize(&snapshot, target.path()).is_err());
}
