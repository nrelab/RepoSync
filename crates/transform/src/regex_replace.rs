use regex::Regex;
use reposync_core::Blob;

use crate::glob::compile_all;
use crate::{
    Error, TransformContext, TransformEvent, TransformResult, Transformation,
};

/// Replaces every match of a regular expression within files matching `files`.
#[derive(Debug, Clone)]
pub struct RegexReplace {
    files: Vec<String>,
    pattern: String,
    replacement: String,
}

impl RegexReplace {
    /// Creates a new [`RegexReplace`] transform.
    #[must_use]
    pub fn new(
        files: impl IntoIterator<Item = impl Into<String>>,
        pattern: impl Into<String>,
        replacement: impl Into<String>,
    ) -> Self {
        Self {
            files: files.into_iter().map(Into::into).collect(),
            pattern: pattern.into(),
            replacement: replacement.into(),
        }
    }
}

impl Transformation for RegexReplace {
    fn name(&self) -> &'static str {
        "regex_replace"
    }

    fn apply(&self, ctx: &mut TransformContext) -> Result<TransformResult, Error> {
        let globs = compile_all(&self.files, self.name())?;
        let regex = Regex::new(&self.pattern).map_err(|e| Error::Transform {
            name: self.name().to_owned(),
            message: format!("invalid pattern `{}`: {e}", self.pattern),
        })?;

        let mut changed = 0;
        let mut warnings = Vec::new();
        for entry in ctx.snapshot.files.values_mut() {
            if !globs.iter().any(|g| g.matches(&entry.path)) {
                continue;
            }
            let Ok(text) = std::str::from_utf8(entry.content.content()) else {
                warnings.push(format!(
                    "`{}` is not valid UTF-8; skipped",
                    entry.path
                ));
                continue;
            };
            let new = regex.replace_all(text, &self.replacement).to_string();
            if new != text {
                entry.content = Blob::from_bytes(new.into_bytes());
                changed += 1;
            }
        }

        Ok(TransformResult {
            changed,
            warnings,
            event: TransformEvent::Rewrote { files: changed },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::snapshot_from;

    #[test]
    fn replaces_matches_across_files() {
        let mut ctx = TransformContext::new(snapshot_from(&[
            ("src/lib.rs", b"let x = 0xAB;\nlet y = 0xCD;"),
            ("README.md", b"color 0xFF"),
            ("a.bin", b"\x00\x01\xff"),
        ]));
        let t = RegexReplace::new(["**"], "0x([0-9A-F]+)", "0xDEAD");
        let result = t.apply(&mut ctx).unwrap();
        // lib.rs and README are text + matched; a.bin is matched but binary.
        assert_eq!(result.changed, 2);
        assert_eq!(
            std::str::from_utf8(
                ctx.snapshot.files[&reposync_core::RepoPath::new("src/lib.rs").unwrap()]
                    .content
                    .content()
            )
            .unwrap(),
            "let x = 0xDEAD;\nlet y = 0xDEAD;"
        );
        assert_eq!(
            std::str::from_utf8(
                ctx.snapshot.files[&reposync_core::RepoPath::new("README.md").unwrap()]
                    .content
                    .content()
            )
            .unwrap(),
            "color 0xDEAD"
        );
        // Binary file matching the glob is skipped with a warning.
        assert!(result.warnings.iter().any(|w| w.contains("a.bin")));
    }

    #[test]
    fn capture_groups_propagate() {
        let mut ctx = TransformContext::new(snapshot_from(&[("v.txt", b"v1.2.3")]));
        let t = RegexReplace::new(["**"], r"v(\d+)\.(\d+)\.(\d+)", "v$3.$2.$1");
        t.apply(&mut ctx).unwrap();
        assert_eq!(
            std::str::from_utf8(
                ctx.snapshot.files[&reposync_core::RepoPath::new("v.txt").unwrap()]
                    .content
                    .content()
            )
            .unwrap(),
            "v3.2.1"
        );
    }
}
