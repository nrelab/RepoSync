use std::path::{Path, PathBuf};

use crate::error::PackageError;
use crate::spec::PackageSpec;

#[cfg(test)]
pub(crate) static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Resolve cache root per PRODUCT.md#4.
#[must_use]
pub fn cache_root() -> PathBuf {
    if let Ok(val) = std::env::var("REPOSYNC_CACHE") {
        if !val.trim().is_empty() {
            return PathBuf::from(val);
        }
    }
    if let Ok(val) = std::env::var("XDG_CACHE_HOME") {
        if !val.trim().is_empty() {
            return PathBuf::from(val).join("reposync");
        }
    }
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".cache").join("reposync")
}

/// Layout for a resolved package.
#[must_use]
pub fn package_dir(spec: &PackageSpec, version: &str) -> PathBuf {
    cache_root()
        .join("packages")
        .join(spec.registry.as_str())
        .join(spec.normalized_name())
        .join(version)
        .join("package")
}

/// Sentinel file marking a complete extraction.
fn complete_marker(package_dir: &Path) -> PathBuf {
    package_dir
        .parent()
        .unwrap_or(package_dir)
        .join(".complete")
}

fn lock_path(package_dir: &Path) -> PathBuf {
    package_dir
        .parent()
        .unwrap_or(package_dir)
        .join(".lock")
}

/// Check if a cached entry is complete and valid.
#[must_use]
pub fn is_cached(package_dir: &Path) -> bool {
    complete_marker(package_dir).exists() && package_dir.is_dir()
}

/// Atomically materialize `src_tmp` (already extracted) into `dest`.
///
/// Uses a file lock to serialize concurrent writers.
pub fn materialize(src_tmp: &Path, dest: &Path) -> Result<(), PackageError> {
    let lock_p = lock_path(dest);
    if let Some(parent) = lock_p.parent() {
        std::fs::create_dir_all(parent).map_err(|e| PackageError::Io {
            path: parent.display().to_string(),
            cause: e.to_string(),
        })?;
    }
    // Ensure dest parent exists.
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| PackageError::Io {
            path: parent.display().to_string(),
            cause: e.to_string(),
        })?;
    }

    // If already complete, nothing to do (racing winner).
    if is_cached(dest) {
        return Ok(());
    }

    // Acquire exclusive lock (try, then block with poll).
    let lock_file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&lock_p)
        .map_err(|e| map_io_err(&lock_p, e))?;

    // Try to acquire lock; if would block, poll for completion.
    {
        use fs2::FileExt;
        let mut waited = 0;
        loop {
            match lock_file.try_lock_exclusive() {
                Ok(()) => break,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock && waited < 60 => {
                    std::thread::sleep(std::time::Duration::from_millis(200));
                    waited += 1;
                    if is_cached(dest) {
                        return Ok(());
                    }
                }
                Err(e) => {
                    return Err(PackageError::Io {
                        path: lock_p.display().to_string(),
                        cause: e.to_string(),
                    })
                }
            }
        }

        // Re-check after acquiring lock (another process may have completed).
        if is_cached(dest) {
            let _ = fs2::FileExt::unlock(&lock_file);
            return Ok(());
        }

        // Remove any stale dest.
        if dest.exists() {
            let _ = std::fs::remove_dir_all(dest);
        }

        // Move temp dir into place (atomic rename if same filesystem, else copy).
        if let Err(e) = std::fs::rename(src_tmp, dest) {
            // Cross-device fallback: recursive copy.
            if e.kind() == std::io::ErrorKind::CrossesDevices {
                copy_dir_recursive(src_tmp, dest).map_err(|cause| PackageError::Io {
                    path: dest.display().to_string(),
                    cause,
                })?;
                let _ = std::fs::remove_dir_all(src_tmp);
            } else {
                return Err(PackageError::Io {
                    path: dest.display().to_string(),
                    cause: e.to_string(),
                });
            }
        }

        // Write sentinel.
        std::fs::write(complete_marker(dest), b"ok").map_err(|e| PackageError::Io {
            path: complete_marker(dest).display().to_string(),
            cause: e.to_string(),
        })?;

        let _ = fs2::FileExt::unlock(&lock_file);
    }

    Ok(())
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let ty = entry.file_type().map_err(|e| e.to_string())?;
        let dst_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &dst_path)?;
        } else if ty.is_file() {
            std::fs::copy(entry.path(), &dst_path).map_err(|e| e.to_string())?;
        } else if ty.is_symlink() {
            // For temp extraction we have already sanitized symlinks, but handle.
            let target = std::fs::read_link(entry.path()).map_err(|e| e.to_string())?;
            #[cfg(unix)]
            std::os::unix::fs::symlink(target, &dst_path).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn map_io_err(path: &Path, e: std::io::Error) -> PackageError {
    if e.kind() == std::io::ErrorKind::PermissionDenied {
        PackageError::Permission {
            path: path.display().to_string(),
            cause: e.to_string(),
        }
    } else {
        PackageError::Io {
            path: path.display().to_string(),
            cause: e.to_string(),
        }
    }
}

/// Evict a specific package version.
pub fn evict(spec: &PackageSpec, version: &str) -> Result<bool, PackageError> {
    let dir = package_dir(spec, version);
    let version_dir = dir.parent().unwrap_or(&dir).to_path_buf();
    if version_dir.exists() {
        std::fs::remove_dir_all(&version_dir).map_err(|e| map_io_err(&version_dir, e))?;
        return Ok(true);
    }
    Ok(false)
}

/// Evict all versions of a package.
pub fn evict_all_versions(spec: &PackageSpec) -> Result<usize, PackageError> {
    let base = cache_root()
        .join("packages")
        .join(spec.registry.as_str())
        .join(spec.normalized_name());
    if !base.exists() {
        return Ok(0);
    }
    let mut count = 0;
    for entry in std::fs::read_dir(&base).map_err(|e| map_io_err(&base, e))? {
        let entry = entry.map_err(|e| PackageError::Io {
            path: base.display().to_string(),
            cause: e.to_string(),
        })?;
        if entry.file_type().map_err(|e| PackageError::Io {
            path: entry.path().display().to_string(),
            cause: e.to_string(),
        })?.is_dir()
        {
            std::fs::remove_dir_all(entry.path()).map_err(|e| map_io_err(&entry.path(), e))?;
            count += 1;
        }
    }
    // Remove the name directory if empty.
    let _ = std::fs::remove_dir(&base);
    Ok(count)
}

/// Evict all packages.
pub fn evict_all() -> Result<usize, PackageError> {
    let base = cache_root().join("packages");
    if !base.exists() {
        return Ok(0);
    }
    let mut count = 0;
    for reg_entry in std::fs::read_dir(&base).map_err(|e| map_io_err(&base, e))? {
        let reg_entry = reg_entry.map_err(|e| PackageError::Io {
            path: base.display().to_string(),
            cause: e.to_string(),
        })?;
        if !reg_entry.file_type().map_err(|e| PackageError::Io {
            path: reg_entry.path().display().to_string(),
            cause: e.to_string(),
        })?.is_dir()
        {
            continue;
        }
        for name_entry in std::fs::read_dir(reg_entry.path()).map_err(|e| map_io_err(&reg_entry.path(), e))? {
            let name_entry = name_entry.map_err(|e| PackageError::Io {
                path: reg_entry.path().display().to_string(),
                cause: e.to_string(),
            })?;
            for ver_entry in std::fs::read_dir(name_entry.path()).map_err(|e| map_io_err(&name_entry.path(), e))? {
                let ver_entry = ver_entry.map_err(|e| PackageError::Io {
                    path: name_entry.path().display().to_string(),
                    cause: e.to_string(),
                })?;
                std::fs::remove_dir_all(ver_entry.path()).map_err(|e| map_io_err(&ver_entry.path(), e))?;
                count += 1;
            }
            let _ = std::fs::remove_dir(name_entry.path());
        }
        let _ = std::fs::remove_dir(reg_entry.path());
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::{PackageSpec, Registry};

    #[test]
    fn cache_root_respects_env() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::set_var("REPOSYNC_CACHE", "/tmp/my-cache");
        assert_eq!(cache_root(), PathBuf::from("/tmp/my-cache"));
        std::env::remove_var("REPOSYNC_CACHE");
    }

    #[test]
    fn package_dir_layout() {
        let _g = ENV_LOCK.lock().unwrap();
        let spec = PackageSpec {
            registry: Registry::Npm,
            name: "Zod".to_owned(),
            version: Some("3.22.4".to_owned()),
        };
        // Use env to make deterministic.
        std::env::set_var("REPOSYNC_CACHE", "/tmp/c");
        let p = package_dir(&spec, "3.22.4");
        assert_eq!(p, PathBuf::from("/tmp/c/packages/npm/zod/3.22.4/package"));
        std::env::remove_var("REPOSYNC_CACHE");
    }
}
