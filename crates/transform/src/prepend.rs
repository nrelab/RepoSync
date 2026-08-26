use reposync_core::Blob;

use crate::glob::compile_all;
use crate::{Error, TransformContext, TransformEvent, TransformResult, Transformation};

/// Prepends `content` to the beginning of each file matching `files`.
#[derive(Debug, Clone)]
pub struct Prepend {
    files: Vec<String>,
    content: String,
}

impl Prepend {
    /// Creates a new [`Prepend`] transform.
    #[must_use]
    pub fn new(
        files: impl IntoIterator<Item = impl Into<String>>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            files: files.into_iter().map(Into::into).collect(),
            content: content.into(),
        }
    }
}

impl Transformation for Prepend {
    fn name(&self) -> &'static str {
        "prepend"
    }

    fn apply(&self, ctx: &mut TransformContext) -> Result<TransformResult, Error> {
        let globs = compile_all(&self.files, self.name())?;
        let mut changed = 0;
        let mut warnings = Vec::new();
        if !self.content.is_empty() {
            for entry in ctx.snapshot.files.values_mut() {
                if !globs.iter().any(|g| g.matches(&entry.path)) {
                    continue;
                }
                let Ok(text) = std::str::from_utf8(entry.content.content()) else {
                    warnings.push(format!("`{}` is not valid UTF-8; skipped", entry.path));
                    continue;
                };
                entry.content = Blob::from_bytes(format!("{}{}", self.content, text).into_bytes());
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
    fn prepends_text() {
        let mut ctx = TransformContext::new(snapshot_from(&[("a.txt", b"world")]));
        let t = Prepend::new(["**"], "// header\n");
        let result = t.apply(&mut ctx).unwrap();
        assert_eq!(result.changed, 1);
        assert_eq!(
            std::str::from_utf8(
                ctx.snapshot.files[&reposync_core::RepoPath::new("a.txt").unwrap()]
                    .content
                    .content()
            )
            .unwrap(),
            "// header\nworld"
        );
    }

    #[test]
    fn empty_content_is_a_noop() {
        let mut ctx = TransformContext::new(snapshot_from(&[("a.txt", b"x")]));
        let t = Prepend::new(["**"], "");
        assert_eq!(t.apply(&mut ctx).unwrap().changed, 0);
    }
}
