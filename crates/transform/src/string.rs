use reposync_core::Blob;

use crate::glob::compile_all;
use crate::{Error, TransformContext, TransformEvent, TransformResult, Transformation};

/// String operations to apply to file contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StringOp {
    /// Capitalize the first character of the string.
    Capitalize,
    /// Count occurrences of a substring.
    Count { pattern: String },
    /// Split string into elements/characters.
    Elems,
    /// Check if string ends with a suffix.
    EndsWith { suffix: String },
    /// Find the first occurrence of a substring.
    Find { pattern: String },
    /// Format string with placeholders.
    Format { args: Vec<String> },
    /// Find the index of a substring.
    Index { pattern: String },
    /// Check if string is alphanumeric.
    IsAlnum,
    /// Check if string is alphabetic.
    IsAlpha,
    /// Check if string is all digits.
    IsDigit,
    /// Check if string is all lowercase.
    IsLower,
    /// Check if string is all whitespace.
    IsSpace,
    /// Check if string is titlecased.
    IsTitle,
    /// Check if string is all uppercase.
    IsUpper,
    /// Join elements with a separator.
    Join { separator: String },
    /// Convert string to lowercase.
    Lower,
    /// Strip leading characters (whitespace by default).
    LStrip { chars: Option<String> },
    /// Partition string at first occurrence of separator.
    Partition { sep: String },
    /// Remove a prefix from the string.
    RemovePrefix { prefix: String },
    /// Remove a suffix from the string.
    RemoveSuffix { suffix: String },
    /// Replace occurrences of a pattern.
    Replace { old: String, new: String },
    /// Find the last occurrence of a substring.
    RFind { pattern: String },
    /// Find the last index of a substring.
    RIndex { pattern: String },
    /// Partition string at last occurrence of separator.
    RPartition { sep: String },
    /// Split string from the right.
    RSplit { sep: String, maxsplit: Option<usize> },
    /// Strip trailing characters (whitespace by default).
    RStrip { chars: Option<String> },
    /// Split string by separator.
    Split { sep: String, maxsplit: Option<usize> },
    /// Split string into lines.
    SplitLines,
    /// Check if string starts with a prefix.
    StartsWith { prefix: String },
    /// Strip leading and trailing characters (whitespace by default).
    Strip { chars: Option<String> },
    /// Convert string to titlecase.
    Title,
    /// Convert string to uppercase.
    Upper,
}

/// Apply string operations to files matching a glob pattern.
#[derive(Debug, Clone)]
pub struct StringTransform {
    file: String,
    op: StringOp,
}

impl StringTransform {
    /// Creates a string transform over files matching `file` (a glob).
    #[must_use]
    pub fn new(file: impl Into<String>, op: StringOp) -> Self {
        Self {
            file: file.into(),
            op,
        }
    }

    #[allow(clippy::too_many_lines, clippy::redundant_closure_for_method_calls, clippy::unnecessary_map_or)]
    fn apply_op(&self, text: &str) -> Result<String, Error> {
        match &self.op {
            StringOp::Capitalize => {
                let mut chars = text.chars();
                match chars.next() {
                    Some(first) => {
                        Ok(first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase())
                    }
                    None => Ok(String::new()),
                }
            }
            StringOp::Count { pattern } => {
                let count = text.matches(pattern).count();
                Ok(count.to_string())
            }
            StringOp::Elems => {
                let elems: Vec<String> = text.chars().map(|c| c.to_string()).collect();
                Ok(elems.join("\n"))
            }
            StringOp::EndsWith { suffix } => {
                let result = text.ends_with(suffix);
                Ok(result.to_string())
            }
            StringOp::Find { pattern } => {
                let result = text.find(pattern).map_or("-1".to_string(), |i| i.to_string());
                Ok(result)
            }
            StringOp::Format { args } => {
                let mut result = text.to_string();
                for (i, arg) in args.iter().enumerate() {
                    result = result.replace(&format!("{{{i}}}"), arg);
                }
                Ok(result)
            }
            StringOp::Index { pattern } => {
                text.find(pattern)
                    .map(|i| i.to_string())
                    .ok_or_else(|| Error::Transform {
                        name: "string".to_string(),
                        message: format!("substring '{pattern}' not found"),
                    })
            }
            StringOp::IsAlnum => Ok(text.chars().all(|c| c.is_alphanumeric()).to_string()),
            StringOp::IsAlpha => Ok(text.chars().all(|c| c.is_alphabetic()).to_string()),
            StringOp::IsDigit => Ok(text.chars().all(|c| c.is_ascii_digit()).to_string()),
            StringOp::IsLower => Ok(text.chars().all(|c| c.is_lowercase()).to_string()),
            StringOp::IsSpace => Ok(text.chars().all(|c| c.is_whitespace()).to_string()),
            StringOp::IsTitle => {
                let result = text
                    .split_whitespace()
                    .all(|word| word.chars().next().map_or(false, |c| c.is_uppercase()));
                Ok(result.to_string())
            }
            StringOp::IsUpper => Ok(text.chars().all(|c| c.is_uppercase()).to_string()),
            StringOp::Join { separator } => {
                let lines: Vec<&str> = text.lines().collect();
                Ok(lines.join(separator))
            }
            StringOp::Lower => Ok(text.to_lowercase()),
            StringOp::LStrip { chars } => {
                let chars_to_strip = chars.as_deref().unwrap_or(" \t\n\r");
                Ok(text.trim_start_matches(chars_to_strip).to_string())
            }
            StringOp::Partition { sep } => {
                if let Some(idx) = text.find(sep) {
                    let (before, rest) = text.split_at(idx);
                    let after = &rest[sep.len()..];
                    Ok(format!("{before}\x1E{sep}\x1E{after}"))
                } else {
                    Ok(format!("{}\x1E\x1E{}", text, ""))
                }
            }
            StringOp::RemovePrefix { prefix } => Ok(text.strip_prefix(prefix).unwrap_or(text).to_string()),
            StringOp::RemoveSuffix { suffix } => Ok(text.strip_suffix(suffix).unwrap_or(text).to_string()),
            StringOp::Replace { old, new } => Ok(text.replace(old, new)),
            StringOp::RFind { pattern } => {
                let result = text.rfind(pattern).map_or("-1".to_string(), |i| i.to_string());
                Ok(result)
            }
            StringOp::RIndex { pattern } => {
                text.rfind(pattern)
                    .map(|i| i.to_string())
                    .ok_or_else(|| Error::Transform {
                        name: "string".to_string(),
                        message: format!("substring '{pattern}' not found"),
                    })
            }
            StringOp::RPartition { sep } => {
                if let Some(idx) = text.rfind(sep) {
                    let (before, rest) = text.split_at(idx);
                    let after = &rest[sep.len()..];
                    Ok(format!("{before}\x1E{sep}\x1E{after}"))
                } else {
                    Ok(format!("{}\x1E\x1E{}", text, ""))
                }
            }
            StringOp::RSplit { sep, maxsplit } => {
                let parts: Vec<String> = if let Some(max) = maxsplit {
                    let parts: Vec<&str> = text.split(sep).collect();
                    if parts.len() > *max {
                        let (head, tail) = parts.split_at(parts.len() - max);
                        let joined = head.join(sep);
                        vec![joined].into_iter().chain(tail.iter().map(|s| s.to_string())).collect()
                    } else {
                        parts.into_iter().map(|s| s.to_string()).collect()
                    }
                } else {
                    text.split(sep).map(|s| s.to_string()).collect()
                };
                Ok(parts.join("\n"))
            }
            StringOp::RStrip { chars } => {
                let chars_to_strip = chars.as_deref().unwrap_or(" \t\n\r");
                Ok(text.trim_end_matches(chars_to_strip).to_string())
            }
            StringOp::Split { sep, maxsplit } => {
                let parts: Vec<String> = if let Some(max) = maxsplit {
                    text.splitn(*max + 1, sep).map(|s| s.to_string()).collect()
                } else {
                    text.split(sep).map(|s| s.to_string()).collect()
                };
                Ok(parts.join("\n"))
            }
            StringOp::SplitLines => Ok(text.lines().collect::<Vec<_>>().join("\n")),
            StringOp::StartsWith { prefix } => {
                let result = text.starts_with(prefix);
                Ok(result.to_string())
            }
            StringOp::Strip { chars } => {
                let chars_to_strip: Vec<char> = chars
                    .as_deref()
                    .unwrap_or(" \t\n\r")
                    .chars()
                    .collect();
                Ok(text.trim_matches(&chars_to_strip[..]).to_string())
            }
            StringOp::Title => {
                let result = text
                    .split_whitespace()
                    .map(|word| {
                        let mut chars = word.chars();
                        match chars.next() {
                            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                            None => String::new(),
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                Ok(result)
            }
            StringOp::Upper => Ok(text.to_uppercase()),
        }
    }
}

impl Transformation for StringTransform {
    fn name(&self) -> &'static str {
        "string"
    }

    fn apply(&self, ctx: &mut TransformContext) -> Result<TransformResult, Error> {
        let globs = compile_all(std::slice::from_ref(&self.file), self.name())?;
        let mut changed = 0usize;
        let mut warnings = Vec::new();

        for entry in ctx.snapshot.files.values_mut() {
            if !globs.iter().any(|glob| glob.matches(&entry.path)) {
                continue;
            }
            let Ok(text) = std::str::from_utf8(entry.content.content()) else {
                warnings.push(format!("`{}` is not valid UTF-8; skipped", entry.path));
                continue;
            };
            let new = self.apply_op(text)?;
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
    use reposync_core::{Blob, FileEntry, RepoPath, RepositorySnapshot};

    fn snapshot() -> RepositorySnapshot {
        let mut s = RepositorySnapshot::new();
        s.insert(FileEntry::regular(
            RepoPath::new("test.txt").unwrap(),
            Blob::from_bytes(b"Hello World"),
        ));
        s
    }

    #[test]
    fn test_capitalize() {
        let transform = StringTransform::new("test.txt", StringOp::Capitalize);
        let mut ctx = TransformContext::new(snapshot());
        transform.apply(&mut ctx).unwrap();
        let content = std::str::from_utf8(
            ctx.snapshot
                .get(&RepoPath::new("test.txt").unwrap())
                .unwrap()
                .content
                .content(),
        )
        .unwrap();
        assert_eq!(content, "Hello world");
    }

    #[test]
    fn test_lower() {
        let transform = StringTransform::new("test.txt", StringOp::Lower);
        let mut ctx = TransformContext::new(snapshot());
        transform.apply(&mut ctx).unwrap();
        let content = std::str::from_utf8(
            ctx.snapshot
                .get(&RepoPath::new("test.txt").unwrap())
                .unwrap()
                .content
                .content(),
        )
        .unwrap();
        assert_eq!(content, "hello world");
    }

    #[test]
    fn test_upper() {
        let transform = StringTransform::new("test.txt", StringOp::Upper);
        let mut ctx = TransformContext::new(snapshot());
        transform.apply(&mut ctx).unwrap();
        let content = std::str::from_utf8(
            ctx.snapshot
                .get(&RepoPath::new("test.txt").unwrap())
                .unwrap()
                .content
                .content(),
        )
        .unwrap();
        assert_eq!(content, "HELLO WORLD");
    }

    #[test]
    fn test_title() {
        let transform = StringTransform::new("test.txt", StringOp::Title);
        let mut ctx = TransformContext::new(snapshot());
        transform.apply(&mut ctx).unwrap();
        let content = std::str::from_utf8(
            ctx.snapshot
                .get(&RepoPath::new("test.txt").unwrap())
                .unwrap()
                .content
                .content(),
        )
        .unwrap();
        assert_eq!(content, "Hello World");
    }

    #[test]
    fn test_replace() {
        let transform = StringTransform::new(
            "test.txt",
            StringOp::Replace {
                old: "World".to_string(),
                new: "Rust".to_string(),
            },
        );
        let mut ctx = TransformContext::new(snapshot());
        transform.apply(&mut ctx).unwrap();
        let content = std::str::from_utf8(
            ctx.snapshot
                .get(&RepoPath::new("test.txt").unwrap())
                .unwrap()
                .content
                .content(),
        )
        .unwrap();
        assert_eq!(content, "Hello Rust");
    }

    #[test]
    fn test_startswith() {
        let transform = StringTransform::new("test.txt", StringOp::StartsWith {
            prefix: "Hello".to_string(),
        });
        let mut ctx = TransformContext::new(snapshot());
        transform.apply(&mut ctx).unwrap();
        let content = std::str::from_utf8(
            ctx.snapshot
                .get(&RepoPath::new("test.txt").unwrap())
                .unwrap()
                .content
                .content(),
        )
        .unwrap();
        assert_eq!(content, "true");
    }

    #[test]
    fn test_endswith() {
        let transform = StringTransform::new("test.txt", StringOp::EndsWith {
            suffix: "World".to_string(),
        });
        let mut ctx = TransformContext::new(snapshot());
        transform.apply(&mut ctx).unwrap();
        let content = std::str::from_utf8(
            ctx.snapshot
                .get(&RepoPath::new("test.txt").unwrap())
                .unwrap()
                .content
                .content(),
        )
        .unwrap();
        assert_eq!(content, "true");
    }

    #[test]
    fn test_strip() {
        let mut s = RepositorySnapshot::new();
        s.insert(FileEntry::regular(
            RepoPath::new("test.txt").unwrap(),
            Blob::from_bytes(b"  Hello World  "),
        ));
        let transform = StringTransform::new("test.txt", StringOp::Strip { chars: None });
        let mut ctx = TransformContext::new(s);
        transform.apply(&mut ctx).unwrap();
        let content = std::str::from_utf8(
            ctx.snapshot
                .get(&RepoPath::new("test.txt").unwrap())
                .unwrap()
                .content
                .content(),
        )
        .unwrap();
        assert_eq!(content, "Hello World");
    }

    #[test]
    fn test_split() {
        let transform = StringTransform::new(
            "test.txt",
            StringOp::Split {
                sep: " ".to_string(),
                maxsplit: None,
            },
        );
        let mut ctx = TransformContext::new(snapshot());
        transform.apply(&mut ctx).unwrap();
        let content = std::str::from_utf8(
            ctx.snapshot
                .get(&RepoPath::new("test.txt").unwrap())
                .unwrap()
                .content
                .content(),
        )
        .unwrap();
        assert_eq!(content, "Hello\nWorld");
    }

    #[test]
    fn test_join() {
        let mut s = RepositorySnapshot::new();
        s.insert(FileEntry::regular(
            RepoPath::new("test.txt").unwrap(),
            Blob::from_bytes(b"Hello\nWorld"),
        ));
        let transform = StringTransform::new("test.txt", StringOp::Join {
            separator: " ".to_string(),
        });
        let mut ctx = TransformContext::new(s);
        transform.apply(&mut ctx).unwrap();
        let content = std::str::from_utf8(
            ctx.snapshot
                .get(&RepoPath::new("test.txt").unwrap())
                .unwrap()
                .content
                .content(),
        )
        .unwrap();
        assert_eq!(content, "Hello World");
    }

    #[test]
    fn test_removeprefix() {
        let transform = StringTransform::new("test.txt", StringOp::RemovePrefix {
            prefix: "Hello ".to_string(),
        });
        let mut ctx = TransformContext::new(snapshot());
        transform.apply(&mut ctx).unwrap();
        let content = std::str::from_utf8(
            ctx.snapshot
                .get(&RepoPath::new("test.txt").unwrap())
                .unwrap()
                .content
                .content(),
        )
        .unwrap();
        assert_eq!(content, "World");
    }

    #[test]
    fn test_removesuffix() {
        let transform = StringTransform::new("test.txt", StringOp::RemoveSuffix {
            suffix: " World".to_string(),
        });
        let mut ctx = TransformContext::new(snapshot());
        transform.apply(&mut ctx).unwrap();
        let content = std::str::from_utf8(
            ctx.snapshot
                .get(&RepoPath::new("test.txt").unwrap())
                .unwrap()
                .content
                .content(),
        )
        .unwrap();
        assert_eq!(content, "Hello");
    }

    #[test]
    fn test_count() {
        let transform = StringTransform::new("test.txt", StringOp::Count {
            pattern: "l".to_string(),
        });
        let mut ctx = TransformContext::new(snapshot());
        transform.apply(&mut ctx).unwrap();
        let content = std::str::from_utf8(
            ctx.snapshot
                .get(&RepoPath::new("test.txt").unwrap())
                .unwrap()
                .content
                .content(),
        )
        .unwrap();
        assert_eq!(content, "3");
    }

    #[test]
    fn test_find() {
        let transform = StringTransform::new("test.txt", StringOp::Find {
            pattern: "World".to_string(),
        });
        let mut ctx = TransformContext::new(snapshot());
        transform.apply(&mut ctx).unwrap();
        let content = std::str::from_utf8(
            ctx.snapshot
                .get(&RepoPath::new("test.txt").unwrap())
                .unwrap()
                .content
                .content(),
        )
        .unwrap();
        assert_eq!(content, "6");
    }

    #[test]
    fn test_isalpha() {
        let mut s = RepositorySnapshot::new();
        s.insert(FileEntry::regular(
            RepoPath::new("test.txt").unwrap(),
            Blob::from_bytes(b"Hello"),
        ));
        let transform = StringTransform::new("test.txt", StringOp::IsAlpha);
        let mut ctx = TransformContext::new(s);
        transform.apply(&mut ctx).unwrap();
        let content = std::str::from_utf8(
            ctx.snapshot
                .get(&RepoPath::new("test.txt").unwrap())
                .unwrap()
                .content
                .content(),
        )
        .unwrap();
        assert_eq!(content, "true");
    }

    #[test]
    fn test_isdigit() {
        let mut s = RepositorySnapshot::new();
        s.insert(FileEntry::regular(
            RepoPath::new("test.txt").unwrap(),
            Blob::from_bytes(b"12345"),
        ));
        let transform = StringTransform::new("test.txt", StringOp::IsDigit);
        let mut ctx = TransformContext::new(s);
        transform.apply(&mut ctx).unwrap();
        let content = std::str::from_utf8(
            ctx.snapshot
                .get(&RepoPath::new("test.txt").unwrap())
                .unwrap()
                .content
                .content(),
        )
        .unwrap();
        assert_eq!(content, "true");
    }

    #[test]
    fn test_partition() {
        let transform = StringTransform::new("test.txt", StringOp::Partition {
            sep: " ".to_string(),
        });
        let mut ctx = TransformContext::new(snapshot());
        transform.apply(&mut ctx).unwrap();
        let content = std::str::from_utf8(
            ctx.snapshot
                .get(&RepoPath::new("test.txt").unwrap())
                .unwrap()
                .content
                .content(),
        )
        .unwrap();
        assert_eq!(content, "Hello\x1E \x1EWorld");
    }
}
