use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use reposync_core::{Blob, FileEntry, FileMode, RepoPath, RepositorySnapshot};

use crate::Error;

/// Walks `root` recursively and builds a snapshot of its files.
///
/// Directories are implicit (derived from file paths). Regular files,
/// executables, and symlinks are captured; other entry types (fifos, sockets,
/// devices) are ignored. Symlinks are captured as symlinks, not followed.
///
/// # Errors
///
/// Returns an error if `root` is not a directory, an entry cannot be read, or
/// a derived path is invalid (for example, not valid UTF-8).
pub fn ingest(root: &Path) -> Result<RepositorySnapshot, Error> {
    if !root.is_dir() {
        return Err(Error::Unsupported {
            path: root.to_path_buf(),
            reason: "not a directory".to_string(),
        });
    }

    let mut files = BTreeMap::new();
    ingest_dir(root, root, &mut files)?;
    Ok(RepositorySnapshot::from_files(files))
}

fn ingest_dir(
    root: &Path,
    dir: &Path,
    files: &mut BTreeMap<RepoPath, FileEntry>,
) -> Result<(), Error> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            ingest_dir(root, &path, files)?;
        } else if file_type.is_file() || file_type.is_symlink() {
            let relative = path.strip_prefix(root).map_err(|_| Error::Unsupported {
                path: path.clone(),
                reason: "entry escaped the ingest root".to_string(),
            })?;
            let repo_path = to_repo_path(root, relative)?;

            let (content, mode) = if file_type.is_symlink() {
                ingest_symlink(&path)?
            } else {
                let bytes = fs::read(&path)?;
                let mode = if is_executable(&path)? {
                    FileMode::Executable
                } else {
                    FileMode::File
                };
                (Blob::from_bytes(bytes), mode)
            };

            let entry = FileEntry::new(repo_path.clone(), content, mode);
            files.insert(repo_path, entry);
        }
    }
    Ok(())
}

fn ingest_symlink(path: &Path) -> Result<(Blob, FileMode), Error> {
    let target = fs::read_link(path)?;
    let target_str = target.to_str().ok_or_else(|| Error::Unsupported {
        path: path.to_path_buf(),
        reason: "symlink target is not valid UTF-8".to_string(),
    })?;
    Ok((Blob::new(target_str.as_bytes().to_vec()), FileMode::Symlink))
}

#[cfg(unix)]
fn is_executable(path: &Path) -> std::io::Result<bool> {
    use std::os::unix::fs::PermissionsExt;

    Ok(fs::metadata(path)?.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(_path: &Path) -> std::io::Result<bool> {
    Ok(false)
}

fn to_repo_path(root: &Path, relative: &Path) -> Result<RepoPath, Error> {
    let text = relative.to_str().ok_or_else(|| Error::Unsupported {
        path: root.join(relative),
        reason: "path is not valid UTF-8".to_string(),
    })?;
    RepoPath::new(text).map_err(Into::into)
}
