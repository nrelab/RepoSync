use std::collections::BTreeMap;

use reposync_core::{FileEntry, RepoPath};

use crate::{Error, TransformContext, TransformEvent, TransformResult, Transformation};

/// Move a directory subtree to a new location.
///
/// `from` must be a concrete directory path. `to` is the destination
/// directory; `.` or the empty string mean the repository root. Files under
/// `from` are relocated to `to/<relative path>`.
#[derive(Debug, Clone)]
pub struct Move {
    from: String,
    to: String,
}

impl Move {
    /// Creates a subtree move from `from` to `to`.
    #[must_use]
    pub fn new(from: impl Into<String>, to: impl Into<String>) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
        }
    }
}

impl Transformation for Move {
    fn name(&self) -> &'static str {
        "move"
    }

    fn apply(&self, ctx: &mut TransformContext) -> Result<TransformResult, Error> {
        relocate(ctx, &self.from, &self.to, Kind::Move, "move")
    }
}

/// Copy a directory subtree to a new location, keeping the originals.
///
/// Semantics match [`Move`], except the source files remain in place.
#[derive(Debug, Clone)]
pub struct Copy {
    from: String,
    to: String,
}

impl Copy {
    /// Creates a subtree copy from `from` to `to`.
    #[must_use]
    pub fn new(from: impl Into<String>, to: impl Into<String>) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
        }
    }
}

impl Transformation for Copy {
    fn name(&self) -> &'static str {
        "copy"
    }

    fn apply(&self, ctx: &mut TransformContext) -> Result<TransformResult, Error> {
        relocate(ctx, &self.from, &self.to, Kind::Copy, "copy")
    }
}

/// Rename a single file.
#[derive(Debug, Clone)]
pub struct Rename {
    from: String,
    to: String,
}

impl Rename {
    /// Creates a rename from a source file path to a destination path.
    #[must_use]
    pub fn new(from: impl Into<String>, to: impl Into<String>) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
        }
    }
}

impl Transformation for Rename {
    fn name(&self) -> &'static str {
        "rename"
    }

    fn apply(&self, ctx: &mut TransformContext) -> Result<TransformResult, Error> {
        let from = RepoPath::new(&self.from).map_err(Error::Core)?;
        let to = RepoPath::new(&self.to).map_err(Error::Core)?;
        if from == to {
            return Ok(TransformResult {
                changed: 0,
                warnings: vec!["rename source and destination are the same".to_owned()],
                event: TransformEvent::Renamed { from, to },
            });
        }
        let entry = ctx.snapshot.get(&from).cloned().ok_or_else(|| Error::Transform {
            name: "rename".to_owned(),
            message: format!("no file at `{from}`"),
        })?;
        if ctx.snapshot.contains(&to) {
            return Err(Error::Transform {
                name: "rename".to_owned(),
                message: format!("collision: target `{to}` already exists"),
            });
        }
        ctx.snapshot.remove(&from);
        ctx.snapshot.insert(FileEntry {
            path: to.clone(),
            content: entry.content,
            mode: entry.mode,
        });
        Ok(TransformResult {
            changed: 1,
            warnings: Vec::new(),
            event: TransformEvent::Renamed { from, to },
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Move,
    Copy,
}

fn relocate(
    ctx: &mut TransformContext,
    from: &str,
    to: &str,
    kind: Kind,
    name: &str,
) -> Result<TransformResult, Error> {
    let from_path = dir_path(from, name)?;
    let to_root = dir_path_opt(to)?;

    let mut plans: Vec<(RepoPath, RepoPath, FileEntry)> = Vec::new();
    for (path, entry) in &ctx.snapshot.files {
        let Some(relative) = strip_dir_prefix(path, &from_path) else {
            continue;
        };
        let target = match &to_root {
            Some(root) => root.join(&relative).map_err(Error::Core)?,
            None => RepoPath::new(relative).map_err(Error::Core)?,
        };
        plans.push((path.clone(), target, entry.clone()));
    }

    if plans.is_empty() {
        return Ok(TransformResult {
            changed: 0,
            warnings: vec![format!("`{from}` matched no files")],
            event: match kind {
                Kind::Move => TransformEvent::Moved {
                    from: from.to_owned(),
                    to: to.to_owned(),
                    paths: Vec::new(),
                },
                Kind::Copy => TransformEvent::Copied {
                    from: from.to_owned(),
                    to: to.to_owned(),
                    paths: Vec::new(),
                },
            },
        });
    }

    let mut seen_targets: BTreeMap<RepoPath, RepoPath> = BTreeMap::new();
    for (source, target, _) in &plans {
        if let Some(previous) = seen_targets.insert(target.clone(), source.clone()) {
            return Err(Error::Transform {
                name: name.to_owned(),
                message: format!("collision: `{previous}` and `{source}` both map to `{target}`"),
            });
        }
    }

    for (_, target, _) in &plans {
        let is_moved_away = plans.iter().any(|(source, _, _)| source == target);
        if ctx.snapshot.contains(target) && !is_moved_away {
            return Err(Error::Transform {
                name: name.to_owned(),
                message: format!("collision: target `{target}` already exists"),
            });
        }
    }

    if kind == Kind::Move {
        for (source, _, _) in &plans {
            ctx.snapshot.remove(source);
        }
    }
    for (_, target, entry) in &plans {
        ctx.snapshot.insert(FileEntry {
            path: target.clone(),
            content: entry.content.clone(),
            mode: entry.mode,
        });
    }

    let paths: Vec<RepoPath> = plans.iter().map(|(source, _, _)| source.clone()).collect();
    let changed = paths.len();
    let event = match kind {
        Kind::Move => TransformEvent::Moved {
            from: from.to_owned(),
            to: to.to_owned(),
            paths,
        },
        Kind::Copy => TransformEvent::Copied {
            from: from.to_owned(),
            to: to.to_owned(),
            paths,
        },
    };
    Ok(TransformResult {
        changed,
        warnings: Vec::new(),
        event,
    })
}

/// Parse a source directory spec; `.` and empty are not valid sources.
fn dir_path(spec: &str, name: &str) -> Result<RepoPath, Error> {
    if spec == "." || spec.is_empty() {
        return Err(Error::Transform {
            name: name.to_owned(),
            message: format!("`{spec}` is not a valid source directory"),
        });
    }
    RepoPath::new(spec).map_err(Error::Core)
}

/// Parse a destination directory spec, where `.`/empty mean the root.
fn dir_path_opt(spec: &str) -> Result<Option<RepoPath>, Error> {
    if spec == "." || spec.is_empty() {
        Ok(None)
    } else {
        Ok(Some(RepoPath::new(spec).map_err(Error::Core)?))
    }
}

/// The relative path under `prefix`, if `path` is strictly inside `prefix`.
fn strip_dir_prefix(path: &RepoPath, prefix: &RepoPath) -> Option<String> {
    let rest = path.as_str().strip_prefix(prefix.as_str())?;
    rest.strip_prefix('/').map(ToOwned::to_owned)
}
