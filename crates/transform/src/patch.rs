use reposync_core::Blob;

use crate::glob::compile_all;
use crate::{Error, TransformContext, TransformEvent, TransformResult, Transformation};

/// Applies a unified diff (`patch`) to the file(s) matching `file`.
#[derive(Debug, Clone)]
pub struct Patch {
    file: String,
    patch: String,
}

impl Patch {
    /// Creates a new [`Patch`] transform.
    #[must_use]
    pub fn new(file: impl Into<String>, patch: impl Into<String>) -> Self {
        Self {
            file: file.into(),
            patch: patch.into(),
        }
    }
}

impl Transformation for Patch {
    fn name(&self) -> &'static str {
        "patch"
    }

    fn apply(&self, ctx: &mut TransformContext) -> Result<TransformResult, Error> {
        let globs = compile_all(std::slice::from_ref(&self.file), self.name())?;
        let mut changed = 0;
        let mut warnings = Vec::new();
        for entry in ctx.snapshot.files.values_mut() {
            if !globs.iter().any(|g| g.matches(&entry.path)) {
                continue;
            }
            let Ok(text) = std::str::from_utf8(entry.content.content()) else {
                warnings.push(format!("`{}` is not valid UTF-8; skipped", entry.path));
                continue;
            };
            match apply_unified_diff(text, &self.patch) {
                Ok(new) if new != text => {
                    entry.content = Blob::from_bytes(new.into_bytes());
                    changed += 1;
                }
                Ok(_) => {}
                Err(e) => warnings.push(format!("`{}`: {e}", entry.path)),
            }
        }
        Ok(TransformResult {
            changed,
            warnings,
            event: TransformEvent::Rewrote { files: changed },
        })
    }
}

#[derive(Debug, PartialEq)]
enum Op<'a> {
    Context(&'a str),
    Add(&'a str),
    Remove(&'a str),
}

struct Hunk<'a> {
    old_start: usize,
    old_count: usize,
    #[allow(dead_code)]
    new_start: usize,
    #[allow(dead_code)]
    new_count: usize,
    ops: Vec<Op<'a>>,
}

fn parse_range(s: &str) -> Result<(usize, usize), String> {
    let s = s.trim_start_matches(['-', '+']).trim();
    if let Some((start, count)) = s.split_once(',') {
        let start: usize = start
            .trim()
            .parse()
            .map_err(|_| "invalid hunk range start".to_string())?;
        let count: usize = count
            .trim()
            .parse()
            .map_err(|_| "invalid hunk range count".to_string())?;
        Ok((start, count))
    } else {
        let start: usize = s
            .parse()
            .map_err(|_| "invalid hunk range start".to_string())?;
        Ok((start, 1))
    }
}

fn parse_hunks(patch: &str) -> Result<Vec<Hunk<'_>>, String> {
    let lines: Vec<&str> = patch.split('\n').collect();
    let mut hunks = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if line.starts_with("@@") {
            let inner = line
                .strip_prefix("@@")
                .unwrap_or("")
                .trim_start()
                .trim_end_matches("@@")
                .trim();
            let (old_part, new_part) = inner
                .split_once('+')
                .ok_or_else(|| "malformed hunk header".to_string())?;
            let old = parse_range(old_part)?;
            let new = parse_range(new_part)?;
            let mut ops = Vec::new();
            i += 1;
            while i < lines.len() && !lines[i].starts_with("@@") {
                let l = lines[i];
                if let Some(c) = l.strip_prefix(' ') {
                    ops.push(Op::Context(c));
                } else if let Some(c) = l.strip_prefix('+') {
                    ops.push(Op::Add(c));
                } else if let Some(c) = l.strip_prefix('-') {
                    ops.push(Op::Remove(c));
                } else if l.starts_with('\\') {
                    // "\ No newline at end of file" marker — ignored.
                } else if l.is_empty() {
                    // Blank line outside any hunk; ignore.
                } else {
                    return Err(format!("unexpected patch line: {l}"));
                }
                i += 1;
            }
            hunks.push(Hunk {
                old_start: old.0,
                old_count: old.1,
                new_start: new.0,
                new_count: new.1,
                ops,
            });
        } else {
            i += 1;
        }
    }
    Ok(hunks)
}

/// Applies a unified diff to `original`, returning the patched text.
fn apply_unified_diff(original: &str, patch: &str) -> Result<String, String> {
    let orig: Vec<&str> = original.split('\n').collect();
    let hunks = parse_hunks(patch)?;
    let mut out: Vec<String> = Vec::new();
    let mut orig_idx: usize = 0;

    for hunk in &hunks {
        let hunk_start = hunk.old_start.saturating_sub(1);
        if hunk_start > orig_idx {
            for line in &orig[orig_idx..hunk_start] {
                out.push(line.to_string());
            }
            orig_idx = hunk_start;
        } else if hunk_start < orig_idx {
            return Err("hunks are out of order or overlapping".to_string());
        }

        for op in &hunk.ops {
            match op {
                Op::Context(c) => {
                    if orig_idx >= orig.len() || orig[orig_idx] != *c {
                        return Err(format!(
                            "context mismatch near original line {}: expected `{c}`, found `{}`",
                            orig_idx + 1,
                            orig.get(orig_idx).unwrap_or(&"<eof>")
                        ));
                    }
                    out.push(c.to_string());
                    orig_idx += 1;
                }
                Op::Remove(r) => {
                    if orig_idx >= orig.len() || orig[orig_idx] != *r {
                        return Err(format!(
                            "remove mismatch near original line {}: expected `{r}`, found `{}`",
                            orig_idx + 1,
                            orig.get(orig_idx).unwrap_or(&"<eof>")
                        ));
                    }
                    orig_idx += 1;
                }
                Op::Add(a) => out.push(a.to_string()),
            }
        }
        orig_idx = hunk_start + hunk.old_count;
    }

    for line in &orig[orig_idx..] {
        out.push(line.to_string());
    }
    Ok(out.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::snapshot_from;

    #[test]
    fn applies_simple_context_patch() {
        let original = "line1\nline2\nline3\nline4\n";
        let patch = "@@ -2,2 +2,2 @@\n line2\n-line3\n+LINE3\n";
        let patched = apply_unified_diff(original, patch).unwrap();
        assert_eq!(patched, "line1\nline2\nLINE3\nline4\n");
    }

    #[test]
    fn applies_insertion_at_start() {
        let original = "b\nc\n";
        let patch = "@@ -0,0 +1,1 @@\n+a\n";
        let patched = apply_unified_diff(original, patch).unwrap();
        assert_eq!(patched, "a\nb\nc\n");
    }

    #[test]
    fn rejects_context_mismatch() {
        let original = "a\nb\nc\n";
        let patch = "@@ -1,1 +1,1 @@\n-z\n";
        assert!(apply_unified_diff(original, patch).is_err());
    }

    #[test]
    fn patch_transform_rewrites_matching_files() {
        let mut ctx = TransformContext::new(snapshot_from(&[("f.txt", b"one\ntwo\nthree\n")]));
        let t = Patch::new(
            "f.txt",
            "@@ -1,3 +1,3 @@\n one\n-two\n+2\n three\n",
        );
        let result = t.apply(&mut ctx).unwrap();
        assert_eq!(result.changed, 1);
        assert_eq!(
            std::str::from_utf8(
                ctx.snapshot.files[&reposync_core::RepoPath::new("f.txt").unwrap()]
                    .content
                    .content()
            )
            .unwrap(),
            "one\n2\nthree\n"
        );
    }
}
