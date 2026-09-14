use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use crate::error::PackageError;

/// Extract a tarball (gzipped) or zip archive to `dest`.
///
/// `archive_path` is the downloaded file (`.tgz`, `.tar.gz`, `.zip`).
/// `package_name`/`version` are used for error messages.
pub fn extract_archive(
    archive_path: &Path,
    dest: &Path,
    package_name: &str,
    version: &str,
) -> Result<(), PackageError> {
    // Detect by extension / magic.
    let bytes = fs::read(archive_path).map_err(|e| PackageError::Io {
        path: archive_path.display().to_string(),
        cause: e.to_string(),
    })?;

    // If zip magic (PK\x03\x04), treat as zip.
    if bytes.starts_with(b"PK\x03\x04") || archive_path.extension().is_some_and(|e| e == "zip") {
        return extract_zip(archive_path, dest, package_name, version);
    }

    // Otherwise treat as tar.gz / tgz.
    extract_tar_gz(archive_path, dest, package_name, version)
}

fn extract_tar_gz(
    archive_path: &Path,
    dest: &Path,
    package_name: &str,
    version: &str,
) -> Result<(), PackageError> {
    let file = fs::File::open(archive_path).map_err(|e| PackageError::Io {
        path: archive_path.display().to_string(),
        cause: e.to_string(),
    })?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(gz);

    fs::create_dir_all(dest).map_err(|e| PackageError::Io {
        path: dest.display().to_string(),
        cause: e.to_string(),
    })?;

    for entry in archive.entries().map_err(|e| PackageError::Extraction {
        name: package_name.to_owned(),
        version: version.to_owned(),
        cause: e.to_string(),
    })? {
        let mut entry = entry.map_err(|e| PackageError::Extraction {
            name: package_name.to_owned(),
            version: version.to_owned(),
            cause: e.to_string(),
        })?;
        let entry_path = entry.path().map_err(|e| PackageError::Extraction {
            name: package_name.to_owned(),
            version: version.to_owned(),
            cause: e.to_string(),
        })?.into_owned();

        // Sandbox: reject absolute, .., and ensure within dest.
        let sanitized = sanitize_entry_path(&entry_path).map_err(|cause| PackageError::Extraction {
            name: package_name.to_owned(),
            version: version.to_owned(),
            cause,
        })?;

        // Strip top-level `package/` prefix that npm tarballs use.
        let relative = strip_npm_package_prefix(&sanitized);

        let dest_path = dest.join(relative);

        // Ensure dest_path is within dest (defense in depth).
        if !is_within(dest, &dest_path) {
            return Err(PackageError::Extraction {
                name: package_name.to_owned(),
                version: version.to_owned(),
                cause: format!("entry escapes destination: {}", entry_path.display()),
            });
        }

        let entry_type = entry.header().entry_type();
        let mode = entry.header().mode().unwrap_or(0o644);
        if entry_type.is_dir() {
            fs::create_dir_all(&dest_path).map_err(|e| PackageError::Io {
                path: dest_path.display().to_string(),
                cause: e.to_string(),
            })?;
        } else if entry_type.is_symlink() || entry_type.is_hard_link() {
            // Skip symlinks that would escape; for safe relative symlinks, recreate.
            if let Some(link) = entry.link_name().map_err(|e| PackageError::Extraction {
                name: package_name.to_owned(),
                version: version.to_owned(),
                cause: e.to_string(),
            })? {
                let link = link.to_string_lossy().to_string();
                // Reject absolute or escaping links.
                if link.starts_with('/') || link.contains("..") {
                    continue;
                }
                // Ensure link target would stay within dest.
                let target = dest_path.parent().unwrap_or(dest).join(&link);
                if !is_within(dest, &target) {
                    continue;
                }
                #[cfg(unix)]
                {
                    let _ = fs::remove_file(&dest_path);
                    std::os::unix::fs::symlink(&link, &dest_path).map_err(|e| PackageError::Io {
                        path: dest_path.display().to_string(),
                        cause: e.to_string(),
                    })?;
                }
            }
        } else {
            // Regular file.
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent).map_err(|e| PackageError::Io {
                    path: parent.display().to_string(),
                    cause: e.to_string(),
                })?;
            }
            let mut out = fs::File::create(&dest_path).map_err(|e| PackageError::Io {
                path: dest_path.display().to_string(),
                cause: e.to_string(),
            })?;
            io::copy(&mut entry, &mut out).map_err(|e| PackageError::Extraction {
                name: package_name.to_owned(),
                version: version.to_owned(),
                cause: e.to_string(),
            })?;
            // Cap permissions.
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let capped = if mode & 0o111 != 0 { 0o755 } else { 0o644 };
                let _ = fs::set_permissions(&dest_path, fs::Permissions::from_mode(capped));
            }
        }
    }

    Ok(())
}

fn extract_zip(
    archive_path: &Path,
    dest: &Path,
    package_name: &str,
    version: &str,
) -> Result<(), PackageError> {
    let file = fs::File::open(archive_path).map_err(|e| PackageError::Io {
        path: archive_path.display().to_string(),
        cause: e.to_string(),
    })?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| PackageError::Extraction {
        name: package_name.to_owned(),
        version: version.to_owned(),
        cause: e.to_string(),
    })?;

    fs::create_dir_all(dest).map_err(|e| PackageError::Io {
        path: dest.display().to_string(),
        cause: e.to_string(),
    })?;

    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| PackageError::Extraction {
            name: package_name.to_owned(),
            version: version.to_owned(),
            cause: e.to_string(),
        })?;
        let entry_path = Path::new(entry.name()).to_path_buf();
        let sanitized = sanitize_entry_path(&entry_path).map_err(|cause| PackageError::Extraction {
            name: package_name.to_owned(),
            version: version.to_owned(),
            cause,
        })?;
        let dest_path = dest.join(&sanitized);
        if !is_within(dest, &dest_path) {
            return Err(PackageError::Extraction {
                name: package_name.to_owned(),
                version: version.to_owned(),
                cause: format!("entry escapes destination: {}", entry_path.display()),
            });
        }

        if entry.is_dir() {
            fs::create_dir_all(&dest_path).map_err(|e| PackageError::Io {
                path: dest_path.display().to_string(),
                cause: e.to_string(),
            })?;
        } else {
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent).map_err(|e| PackageError::Io {
                    path: parent.display().to_string(),
                    cause: e.to_string(),
                })?;
            }
            let mut out = fs::File::create(&dest_path).map_err(|e| PackageError::Io {
                path: dest_path.display().to_string(),
                cause: e.to_string(),
            })?;
            io::copy(&mut entry, &mut out).map_err(|e| PackageError::Extraction {
                name: package_name.to_owned(),
                version: version.to_owned(),
                cause: e.to_string(),
            })?;
        }
    }

    Ok(())
}

fn sanitize_entry_path(p: &Path) -> Result<PathBuf, String> {
    if p.is_absolute() {
        return Err(format!("absolute path not allowed: {}", p.display()));
    }
    let mut out = PathBuf::new();
    for comp in p.components() {
        match comp {
            Component::ParentDir => return Err(format!("'..' not allowed: {}", p.display())),
            Component::RootDir | Component::Prefix(_) => {
                return Err(format!("absolute/prefix not allowed: {}", p.display()))
            }
            Component::Normal(c) => out.push(c),
            Component::CurDir => {}
        }
    }
    Ok(out)
}

fn strip_npm_package_prefix(p: &Path) -> PathBuf {
    // npm tarballs contain `package/<files>`. Strip the leading `package`.
    let mut comps = p.components();
    if let Some(Component::Normal(first)) = comps.next() {
        if first == "package" {
            return comps.as_path().to_path_buf();
        }
    }
    p.to_path_buf()
}

fn is_within(base: &Path, candidate: &Path) -> bool {
    // Compare canonical-ish: both are absolute or both relative under base.
    // We use lexical check: candidate starts_with base.
    // Ensure base is normalized.
    let base = if base.is_absolute() {
        base.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join(base)
    };
    let cand = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join(candidate)
    };
    cand.starts_with(&base)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_absolute_and_parent() {
        assert!(sanitize_entry_path(Path::new("/etc/passwd")).is_err());
        assert!(sanitize_entry_path(Path::new("../../a")).is_err());
        assert!(sanitize_entry_path(Path::new("a/../b")).is_err());
        assert_eq!(
            sanitize_entry_path(Path::new("a/b/c")).unwrap(),
            PathBuf::from("a/b/c")
        );
    }

    #[test]
    fn strips_npm_prefix() {
        assert_eq!(
            strip_npm_package_prefix(Path::new("package/src/index.ts")),
            PathBuf::from("src/index.ts")
        );
        assert_eq!(
            strip_npm_package_prefix(Path::new("src/index.ts")),
            PathBuf::from("src/index.ts")
        );
    }

    #[test]
    fn roundtrip_tar_gz() {
        let tmp = tempfile::tempdir().unwrap();
        let archive = tmp.path().join("test.tgz");
        let dest = tmp.path().join("dest");
        // Create a simple tar.gz with `package/hello.txt`.
        {
            let file = fs::File::create(&archive).unwrap();
            let gz = flate2::write::GzEncoder::new(file, flate2::Compression::default());
            let mut tar = tar::Builder::new(gz);
            let mut header = tar::Header::new_gnu();
            header.set_path("package/hello.txt").unwrap();
            header.set_size(5);
            header.set_mode(0o644);
            header.set_cksum();
            tar.append(&header, b"hello".as_slice()).unwrap();
            tar.finish().unwrap();
        }
        extract_archive(&archive, &dest, "test", "1.0.0").unwrap();
        assert_eq!(fs::read_to_string(dest.join("hello.txt")).unwrap(), "hello");
    }
}
