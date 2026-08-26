use std::fs;
use std::path::Path;

use reposync_core::{FileMode, RepositorySnapshot};

use crate::Error;

/// Writes a snapshot into `dir`, deterministically and exactly.
///
/// Any existing contents of `dir` are removed first so that the result matches
/// the snapshot precisely. Files are written in sorted path order. Symlinks
/// require a Unix platform; gitlink (submodule) entries cannot be materialized.
///
/// # Errors
///
/// Returns an error if `dir` exists and is not a directory, a write or
/// permission change fails, a symlink target is not valid UTF-8, or the
/// snapshot contains a gitlink entry.
pub fn materialize(snapshot: &RepositorySnapshot, dir: &Path) -> Result<(), Error> {
    if dir.exists() {
        if !dir.is_dir() {
            return Err(Error::Unsupported {
                path: dir.to_path_buf(),
                reason: "target exists and is not a directory".to_string(),
            });
        }
        remove_dir_contents(dir)?;
    } else {
        fs::create_dir_all(dir)?;
    }

    for (path, entry) in snapshot.iter() {
        let target = dir.join(path.as_path());
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }

        match entry.mode {
            FileMode::Symlink => materialize_symlink(&target, entry.content.content())?,
            FileMode::Gitlink => {
                return Err(Error::Unsupported {
                    path: target,
                    reason: "gitlink (submodule) entries cannot be materialized".to_string(),
                });
            }
            FileMode::File | FileMode::Executable => {
                fs::write(&target, entry.content.content())?;
                if entry.mode == FileMode::Executable {
                    set_executable(&target)?;
                }
            }
        }
    }

    Ok(())
}

fn remove_dir_contents(dir: &Path) -> Result<(), Error> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            fs::remove_dir_all(&path)?;
        } else {
            fs::remove_file(&path)?;
        }
    }
    Ok(())
}

fn materialize_symlink(target_path: &Path, target: &[u8]) -> Result<(), Error> {
    let target_str = std::str::from_utf8(target).map_err(|_| Error::Unsupported {
        path: target_path.to_path_buf(),
        reason: "symlink target is not valid UTF-8".to_string(),
    })?;

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target_str, target_path)?;
    }
    #[cfg(not(unix))]
    {
        let _ = target_str;
        return Err(Error::Unsupported {
            path: target_path.to_path_buf(),
            reason: "symlinks are not supported on this platform".to_string(),
        });
    }
    Ok(())
}

#[cfg(unix)]
fn set_executable(path: &Path) -> Result<(), Error> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> Result<(), Error> {
    Ok(())
}
