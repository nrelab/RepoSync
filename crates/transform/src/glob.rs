use reposync_core::RepoPath;

use crate::Error;

/// A `/`-separated glob pattern with `*`, `?`, and `**` support.
///
/// - `*` matches any run of characters within one path segment.
/// - `?` matches exactly one character within a segment.
/// - `**` matches any number of whole segments (including zero).
///
/// A pattern must match the full repository path: a pattern without a `/`
/// only matches a root-level file. Use `**/name` to match a basename at any
/// depth.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Glob {
    pattern: String,
    segments: Vec<String>,
}

impl Glob {
    /// Parses a glob pattern.
    ///
    /// # Errors
    ///
    /// Returns an error for empty patterns, patterns with empty segments
    /// (e.g. `a//b` or a leading/trailing `/`), or patterns containing NUL
    /// bytes.
    pub fn new(pattern: impl Into<String>) -> Result<Self, Error> {
        let pattern = pattern.into();
        if pattern.is_empty() {
            return Err(Error::InvalidPattern {
                pattern,
                message: "pattern must not be empty",
            });
        }
        if pattern.contains('\0') {
            return Err(Error::InvalidPattern {
                pattern,
                message: "pattern must not contain NUL bytes",
            });
        }
        if pattern.split('/').any(str::is_empty) {
            return Err(Error::InvalidPattern {
                pattern,
                message: "pattern must not contain empty segments",
            });
        }
        let segments: Vec<String> = pattern.split('/').map(str::to_owned).collect();
        Ok(Self { pattern, segments })
    }

    /// The original pattern string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.pattern
    }

    /// Whether `path` matches this pattern.
    #[must_use]
    pub fn matches(&self, path: &RepoPath) -> bool {
        let pattern: Vec<&str> = self.segments.iter().map(String::as_str).collect();
        let text: Vec<&str> = path.as_str().split('/').collect();
        match_segments(&pattern, &text)
    }
}

/// Compile several patterns, tagging parse errors with the transform name.
pub(crate) fn compile_all(patterns: &[String], name: &str) -> Result<Vec<Glob>, Error> {
    patterns
        .iter()
        .map(|pattern| {
            Glob::new(pattern).map_err(|error| Error::Transform {
                name: name.to_owned(),
                message: error.to_string(),
            })
        })
        .collect()
}

/// Match path segments against pattern segments, honouring `**`.
fn match_segments(pattern: &[&str], text: &[&str]) -> bool {
    let Some((&head, pattern_tail)) = pattern.split_first() else {
        return text.is_empty();
    };
    if head == "**" {
        if match_segments(pattern_tail, text) {
            return true;
        }
        let Some((_, text_tail)) = text.split_first() else {
            return false;
        };
        return match_segments(pattern, text_tail);
    }
    let Some((&text_head, text_tail)) = text.split_first() else {
        return false;
    };
    segment_match(head, text_head) && match_segments(pattern_tail, text_tail)
}

/// Match one path segment against one pattern segment (`*` and `?`).
fn segment_match(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let mut star: Option<(usize, usize)> = None;
    while ti < text.len() {
        if pi < pattern.len() && (pattern[pi] == '?' || pattern[pi] == text[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < pattern.len() && pattern[pi] == '*' {
            star = Some((pi, ti));
            pi += 1;
        } else if let Some((sp, st)) = star {
            pi = sp;
            ti = st + 1;
            star = Some((sp, st + 1));
        } else {
            return false;
        }
    }
    while pi < pattern.len() && pattern[pi] == '*' {
        pi += 1;
    }
    pi == pattern.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(p: &str) -> RepoPath {
        RepoPath::new(p).unwrap()
    }

    #[test]
    fn accepts_valid_patterns() {
        for pattern in ["*", "**", "packages/sdk/**", "**/secrets/**", "src/*.rs", "a?c.txt"] {
            assert!(Glob::new(pattern).is_ok(), "expected `{pattern}` to be valid");
        }
    }

    #[test]
    fn rejects_invalid_patterns() {
        for pattern in ["", "a//b", "/abs", "trailing/", "\0", "a\0b"] {
            assert!(Glob::new(pattern).is_err(), "expected `{pattern}` to be rejected");
        }
    }

    #[test]
    fn star_matches_within_a_segment() {
        let glob = Glob::new("src/*.rs").unwrap();
        assert!(glob.matches(&path("src/lib.rs")));
        assert!(glob.matches(&path("src/mod.rs")));
        assert!(!glob.matches(&path("src/deep/lib.rs")));
        assert!(!glob.matches(&path("lib.rs")));
    }

    #[test]
    fn question_mark_matches_one_char() {
        let glob = Glob::new("a?c.txt").unwrap();
        assert!(glob.matches(&path("abc.txt")));
        assert!(!glob.matches(&path("abbc.txt")));
    }

    #[test]
    fn double_star_crosses_segments() {
        let glob = Glob::new("packages/sdk/**").unwrap();
        assert!(glob.matches(&path("packages/sdk")));
        assert!(glob.matches(&path("packages/sdk/lib.rs")));
        assert!(glob.matches(&path("packages/sdk/src/deep/mod.rs")));
        assert!(!glob.matches(&path("packages/other/lib.rs")));

        let glob = Glob::new("**/secrets/**").unwrap();
        assert!(glob.matches(&path("config/secrets/api.key")));
        assert!(glob.matches(&path("secrets/key")));
        assert!(!glob.matches(&path("config/secrets.txt")));
    }

    #[test]
    fn pattern_without_slash_matches_root_only() {
        let glob = Glob::new("*.md").unwrap();
        assert!(glob.matches(&path("README.md")));
        assert!(!glob.matches(&path("docs/guide.md")));
    }

    #[test]
    fn double_star_alone_matches_everything() {
        let glob = Glob::new("**").unwrap();
        assert!(glob.matches(&path("a")));
        assert!(glob.matches(&path("a/b/c.txt")));
    }
}
