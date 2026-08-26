use serde::{Deserialize, Serialize};

/// Where a pipeline reads from.
///
/// Providers are distinguished by the `type` tag. Only `git` is supported so
/// far; GitHub/GitLab/Gerrit providers will be added later behind the same
/// `GitRepository` abstraction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Source {
    /// A Git repository (local, SSH, or HTTPS).
    Git {
        /// Repository location: local path, `git@host:path`, or a URL.
        url: String,
        /// Ref (branch, tag, or commit) to check out.
        #[serde(rename = "ref")]
        ref_name: String,
    },
}

/// Where a pipeline writes to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Destination {
    /// A Git repository (local, SSH, or HTTPS).
    Git {
        /// Repository location: local path, `git@host:path`, or a URL.
        url: String,
        /// Branch to push the result to.
        branch: String,
    },
}

impl Source {
    /// Returns the provider type name (`"git"`).
    #[must_use]
    pub const fn type_name(&self) -> &'static str {
        match self {
            Self::Git { .. } => "git",
        }
    }

    /// Returns the repository URL.
    #[must_use]
    pub fn url(&self) -> &str {
        match self {
            Self::Git { url, .. } => url,
        }
    }
}

impl Destination {
    /// Returns the provider type name (`"git"`).
    #[must_use]
    pub const fn type_name(&self) -> &'static str {
        match self {
            Self::Git { .. } => "git",
        }
    }

    /// Returns the repository URL.
    #[must_use]
    pub fn url(&self) -> &str {
        match self {
            Self::Git { url, .. } => url,
        }
    }
}

/// Returns `true` if `url` looks like a usable git location.
///
/// Accepts scp-like SSH (`git@host:path`), `ssh://`, `https://`, `http://`,
/// `git://`, `file://`, and local filesystem paths.
#[must_use]
pub fn is_valid_git_url(url: &str) -> bool {
    let url = url.trim();
    if url.is_empty() {
        return false;
    }

    if let Some(rest) = url.strip_prefix("git@") {
        return rest.contains(':') && !rest.starts_with(':');
    }

    if let Some(separator) = url.find("://") {
        let scheme = &url[..separator];
        let valid_scheme = matches!(scheme, "ssh" | "https" | "http" | "git" | "file");
        if !valid_scheme {
            return false;
        }
        let after = &url[separator + "://".len()..];
        return !after.is_empty();
    }

    url.starts_with('/')
        || url.starts_with("./")
        || url.starts_with("../")
        || url.starts_with("~/")
        || url == "~"
}

/// Normalizes a URL for equality comparisons: trims whitespace, strips a
/// trailing `/` and a trailing `.git` suffix.
#[must_use]
pub fn normalize_url(url: &str) -> String {
    url.trim()
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_common_git_locations() {
        for url in [
            "git@github.com:org/repo.git",
            "ssh://git@github.com/org/repo.git",
            "https://github.com/org/repo.git",
            "http://host/repo.git",
            "git://host/repo.git",
            "file:///tmp/repo",
            "/abs/path/to/repo",
            "./relative",
            "../up/repo",
            "~/code/repo",
        ] {
            assert!(is_valid_git_url(url), "expected `{url}` to be valid");
        }
    }

    #[test]
    fn rejects_non_git_locations() {
        for url in [
            "",
            "   ",
            "not a url",
            "https://",
            "git@:",
            "git@host",
            "foo",
        ] {
            assert!(!is_valid_git_url(url), "expected `{url}` to be rejected");
        }
    }

    #[test]
    fn normalization_makes_equivalence() {
        assert_eq!(normalize_url("git@h:o/repo.git"), "git@h:o/repo");
        assert_eq!(normalize_url("git@h:o/repo.git/"), "git@h:o/repo");
        assert_eq!(normalize_url(" https://h/o.git "), "https://h/o");
        assert_ne!(normalize_url("a/repo"), normalize_url("b/repo"));
    }

    #[test]
    fn type_names() {
        let source = Source::Git {
            url: "u".into(),
            ref_name: "main".into(),
        };
        assert_eq!(source.type_name(), "git");
    }
}
