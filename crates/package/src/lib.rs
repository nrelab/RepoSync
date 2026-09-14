//! Package source materialization for RepoSync.
//!
//! Resolves `npm`/`pypi` package specs to a local directory under the
//! reposync cache, fetching and verifying tarballs on cache miss.

#![allow(clippy::pedantic)]
#![allow(clippy::missing_errors_doc, clippy::missing_panics_doc)]
#![allow(clippy::new_without_default, clippy::should_implement_trait)]

mod cache;
mod error;
mod extract;
mod registry;
mod spec;

pub use cache::{cache_root, evict, evict_all, evict_all_versions, is_cached, package_dir};
pub use error::PackageError;
pub use registry::{NpmClient, PypiClient, RegistryClient, ResolvedPackage};
pub use spec::{PackageSpec, Registry};

use std::path::PathBuf;

/// Resolve `spec_str` (with optional `--version` override) to a local directory.
///
/// On cache hit, returns the path without network I/O. On miss, fetches from
/// the registry, verifies integrity, extracts and atomically materializes.
///
/// `spec_str` examples: `zod`, `zod@3.22.4`, `pypi:requests==2.31.0`.
pub fn resolve_and_materialize(
    spec_str: &str,
    version_override: Option<&str>,
    verbose: bool,
) -> Result<PathBuf, PackageError> {
    let spec = PackageSpec::parse(spec_str, version_override)?;

    // If version is known and cache is warm, return immediately (offline-friendly).
    if let Some(ver) = &spec.version {
        let dir = package_dir(&spec, ver);
        if is_cached(&dir) {
            if verbose {
                eprintln!("reposync: using cache {}@{} -> {}", spec.name, ver, dir.display());
            }
            return Ok(dir);
        }
    }

    // Need to resolve version if not pinned.
    let client: Box<dyn RegistryClient> = match spec.registry {
        Registry::Npm => Box::new(NpmClient::new()),
        Registry::Pypi => Box::new(PypiClient::new()),
    };

    let resolved = client.resolve(&spec)?;

    let dir = package_dir(&spec, &resolved.version);
    if is_cached(&dir) {
        if verbose {
            eprintln!(
                "reposync: using cache {}@{} -> {}",
                spec.name,
                resolved.version,
                dir.display()
            );
        }
        return Ok(dir);
    }

    if verbose {
        eprintln!(
            "reposync: fetching {}@{} from {}",
            spec.name, resolved.version, resolved.tarball_url
        );
    }

    // Fetch to temp file.
    let tmp_dir = tempfile::tempdir().map_err(|e| PackageError::Io {
        path: std::env::temp_dir().display().to_string(),
        cause: e.to_string(),
    })?;
    let tmp_file = tmp_dir.path().join("package.tgz");
    client.fetch(&resolved, &tmp_file)?;

    // Extract to temp dir.
    let extract_tmp = tempfile::tempdir().map_err(|e| PackageError::Io {
        path: std::env::temp_dir().display().to_string(),
        cause: e.to_string(),
    })?;
    let extract_dest = extract_tmp.path().join("package");
    crate::extract::extract_archive(&tmp_file, &extract_dest, &spec.name, &resolved.version)?;

    // Materialize atomically.
    crate::cache::materialize(&extract_dest, &dir)?;

    // Verify dir exists.
    if !dir.exists() {
        return Err(PackageError::Io {
            path: dir.display().to_string(),
            cause: "materialized directory missing".to_owned(),
        });
    }

    if verbose {
        eprintln!("reposync: resolved {} -> {}", spec_str, resolved.version);
    }

    Ok(dir)
}

/// Evict cache entries per PRODUCT.md#11-12.
pub fn evict_spec(
    spec_str: &str,
    version_override: Option<&str>,
    all_versions: bool,
    all: bool,
) -> Result<usize, PackageError> {
    if all {
        return evict_all();
    }
    let spec = PackageSpec::parse(spec_str, version_override)?;
    if all_versions {
        return evict_all_versions(&spec);
    }
    if let Some(ver) = &spec.version {
        Ok(if evict(&spec, ver)? { 1 } else { 0 })
    } else {
        // No version pinned: evict all versions of that package.
        evict_all_versions(&spec)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn cached_hit_returns_without_network() {
        let _guard = crate::cache::ENV_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let cache = tmp.path().join("cache");
        std::env::set_var("REPOSYNC_CACHE", &cache);
        // Pre-populate cache for npm:zod@1.2.3
        let spec = PackageSpec::parse("zod@1.2.3", None).unwrap();
        let dir = package_dir(&spec, "1.2.3");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("index.js"), b"hello").unwrap();
        fs::write(dir.parent().unwrap().join(".complete"), b"ok").unwrap();

        // Even with a bogus registry, cache hit should succeed without network.
        std::env::set_var("REPOSYNC_NPM_REGISTRY", "https://invalid.example.invalid");
        let result = resolve_and_materialize("zod@1.2.3", None, false).unwrap();
        assert_eq!(result, dir);
        assert!(result.join("index.js").exists());

        // Cleanup env.
        std::env::remove_var("REPOSYNC_CACHE");
        std::env::remove_var("REPOSYNC_NPM_REGISTRY");
    }

    #[test]
    fn evict_removes_entry() {
        let _guard = crate::cache::ENV_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let cache = tmp.path().join("cache");
        std::env::set_var("REPOSYNC_CACHE", &cache);
        let spec = PackageSpec::parse("pypi:requests==2.31.0", None).unwrap();
        let dir = package_dir(&spec, "2.31.0");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("test.py"), b"x").unwrap();
        fs::write(dir.parent().unwrap().join(".complete"), b"ok").unwrap();
        assert!(is_cached(&dir));

        let evicted = evict_spec("pypi:requests==2.31.0", None, false, false).unwrap();
        assert_eq!(evicted, 1);
        assert!(!dir.exists());

        std::env::remove_var("REPOSYNC_CACHE");
    }
}
