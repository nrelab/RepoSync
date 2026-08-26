use std::fmt;
use std::path::Path;
use std::str::FromStr;

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::Error;

/// A validated, `/`-separated relative repository path.
///
/// Invariants enforced by [`RepoPath::new`]:
///
/// - non-empty
/// - relative (no leading `/`)
/// - no empty segments (no trailing slash, no `//`)
/// - no `.` or `..` segments
/// - no NUL bytes
///
/// Paths are always stored in canonical `/`-separated form regardless of the
/// host platform.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RepoPath {
    inner: String,
}

impl RepoPath {
    /// Creates a path after validating it against the path invariants.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidPath`] if the path violates any invariant.
    pub fn new(path: impl Into<String>) -> Result<Self, Error> {
        let inner = path.into();
        validate(&inner)?;
        Ok(Self { inner })
    }

    /// Returns the path as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.inner
    }

    /// Returns the path as a filesystem path (using `/` separators).
    #[must_use]
    pub fn as_path(&self) -> &Path {
        Path::new(&self.inner)
    }

    /// Returns the parent path, or `None` if the path has no `/`.
    #[must_use]
    pub fn parent(&self) -> Option<RepoPath> {
        self.inner.rsplit_once('/').map(|(parent, _)| {
            let inner = parent.to_string();
            debug_assert!(validate(&inner).is_ok());
            RepoPath { inner }
        })
    }

    /// Returns the final path segment.
    #[must_use]
    pub fn file_name(&self) -> &str {
        self.inner.split('/').next_back().unwrap_or(&self.inner)
    }

    /// Joins a relative segment (or nested path) onto this path.
    ///
    /// A leading `/` on `other` is tolerated and stripped. Joining the empty
    /// string returns a clone.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidPath`] if the joined path violates any invariant.
    pub fn join(&self, other: &str) -> Result<Self, Error> {
        if other.is_empty() {
            return Ok(self.clone());
        }
        let other = other.trim_start_matches('/');
        let joined = if self.inner.is_empty() {
            other.to_string()
        } else {
            format!("{}/{}", self.inner, other)
        };
        Self::new(joined)
    }
}

impl fmt::Display for RepoPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.inner)
    }
}

impl AsRef<str> for RepoPath {
    fn as_ref(&self) -> &str {
        &self.inner
    }
}

impl AsRef<Path> for RepoPath {
    fn as_ref(&self) -> &Path {
        self.as_path()
    }
}

impl FromStr for RepoPath {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl Serialize for RepoPath {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.inner)
    }
}

impl<'de> Deserialize<'de> for RepoPath {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct RepoPathVisitor;

        impl Visitor<'_> for RepoPathVisitor {
            type Value = RepoPath;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a valid relative repository path")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                RepoPath::new(value).map_err(E::custom)
            }
        }

        deserializer.deserialize_str(RepoPathVisitor)
    }
}

fn validate(path: &str) -> Result<(), Error> {
    if path.is_empty() {
        return Err(Error::InvalidPath(
            path.to_string(),
            "path must not be empty",
        ));
    }
    if path.starts_with('/') {
        return Err(Error::InvalidPath(
            path.to_string(),
            "path must be relative",
        ));
    }
    if path.contains('\0') {
        return Err(Error::InvalidPath(
            path.to_string(),
            "path must not contain NUL bytes",
        ));
    }
    for segment in path.split('/') {
        if segment.is_empty() {
            return Err(Error::InvalidPath(
                path.to_string(),
                "path must not contain empty segments",
            ));
        }
        if segment == "." || segment == ".." {
            return Err(Error::InvalidPath(
                path.to_string(),
                "path must not contain '.' or '..' segments",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_paths() {
        for path in [
            "a",
            "a/b/c",
            "foo.txt",
            "dir/file.rs",
            "packages/sdk/src/lib.rs",
        ] {
            assert!(RepoPath::new(path).is_ok(), "expected `{path}` to be valid");
        }
    }

    #[test]
    fn rejects_invalid_paths() {
        for path in [
            "", "/a", "a//b", "a/", "./a", "../a", "a/../b", "a/.", "a/./b", "a\0b",
        ] {
            assert!(
                RepoPath::new(path).is_err(),
                "expected `{path}` to be rejected"
            );
        }
    }

    #[test]
    fn parent_and_file_name() {
        let path = RepoPath::new("a/b/c.rs").unwrap();
        assert_eq!(path.parent(), Some(RepoPath::new("a/b").unwrap()));
        assert_eq!(path.file_name(), "c.rs");

        let single = RepoPath::new("c.rs").unwrap();
        assert_eq!(single.parent(), None);
        assert_eq!(single.file_name(), "c.rs");
    }

    #[test]
    fn join_combines_paths() {
        let base = RepoPath::new("a/b").unwrap();
        assert_eq!(base.join("c/d").unwrap(), RepoPath::new("a/b/c/d").unwrap());
        assert_eq!(base.join("/c").unwrap(), RepoPath::new("a/b/c").unwrap());
        assert_eq!(base.join("").unwrap(), base);
    }

    #[test]
    fn json_round_trip() {
        let path = RepoPath::new("packages/sdk/src/lib.rs").unwrap();
        let json = serde_json::to_string(&path).unwrap();
        assert_eq!(json, "\"packages/sdk/src/lib.rs\"");
        let back: RepoPath = serde_json::from_str(&json).unwrap();
        assert_eq!(back, path);
    }

    #[test]
    fn json_rejects_invalid_path() {
        let result: Result<RepoPath, _> = serde_json::from_str("\"/abs/path\"");
        assert!(result.is_err());
    }
}
