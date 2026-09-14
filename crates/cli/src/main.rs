use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::Context;
use clap::{Parser, Subcommand};
use reposync_config::{ConfigFile, Destination, Source, TransformKind};
use reposync_diff::diff;
use reposync_git::{CommitSpec, GitRepo};
use reposync_migration::{Error as MigrationError, SyncStrategy};
use reposync_policy::check;
use reposync_sync::Syncer;
use reposync_transform::{
    Append, Copy, Delete, DependencyRewrite, Filter, ImportRewrite, Metadata, Move, Patch, Prepend,
    RegexReplace, Rename, Replace, StripPrefix, Transformation,
};
use sha2::{Digest, Sha256};

/// Programmable Git repository migration, synchronization, filtering, and
/// transformation engine.
#[derive(Debug, Parser)]
#[command(
    name = "reposync",
    version,
    about = "Programmable Git repository migration, synchronization, filtering, and transformation engine"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Validate a pipeline configuration file.
    Validate {
        /// Path to the pipeline configuration file.
        config: PathBuf,
    },
    /// Show a dry-run plan for a pipeline (does not modify anything).
    Plan {
        /// Path to the pipeline configuration file.
        config: PathBuf,
    },
    /// Run the pipeline end-to-end: read source, transform, commit, push.
    Migrate {
        /// Path to the pipeline configuration file.
        config: PathBuf,
        /// Path to the sync state database (default: `.sync/state.db`).
        #[arg(long, default_value = ".sync/state.db")]
        state: PathBuf,
        /// Replay the full commit history (one commit per source commit) instead
        /// of snapshot mode (a single commit at HEAD).
        #[arg(long)]
        history: bool,
    },
    /// Show a path/content-level diff between source and destination.
    Diff {
        /// Path to the pipeline configuration file.
        config: PathBuf,
    },
    /// Two-way sync between internal (source) and public (destination) with
    /// loop prevention.
    Sync {
        /// Path to the pipeline configuration file.
        config: PathBuf,
        /// Path to the sync state database (default: `.sync/state.db`).
        #[arg(long, default_value = ".sync/state.db")]
        state: PathBuf,
    },
    /// Resolve a package spec to its unpacked source directory.
    Path {
        /// Package spec: `name`, `name@version`, `npm:name@version`, `pypi:name==version`.
        spec: Option<String>,
        /// Override registry (`npm` or `pypi`).
        #[arg(long, short = 'r')]
        registry: Option<String>,
        /// Override version.
        #[arg(long, short = 'v')]
        version: Option<String>,
        /// Print JSON `{"registry","name","version","path","cached"}` instead of bare path.
        #[arg(long)]
        json: bool,
        /// Verbose progress to stderr.
        #[arg(long, short, action = clap::ArgAction::Count)]
        verbose: u8,
        /// Evict cached entry instead of resolving.
        #[arg(long)]
        evict: bool,
        /// With --evict, evict all versions of the package.
        #[arg(long)]
        all_versions: bool,
        /// With --evict, evict all packages.
        #[arg(long)]
        all: bool,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Validate { config } => cmd_validate(&config),
        Command::Plan { config } => cmd_plan(&config),
        Command::Migrate {
            config,
            state,
            history,
        } => cmd_migrate(&config, &state, history),
        Command::Diff { config } => cmd_diff(&config),
        Command::Sync { config, state } => cmd_sync(&config, &state),
        Command::Path {
            spec,
            registry,
            version,
            json,
            verbose,
            evict,
            all_versions,
            all,
        } => cmd_path(
            spec, registry, version, json, verbose, evict, all_versions, all,
        ),
    }
}

// ---------------------------------------------------------------------------
// validate
// ---------------------------------------------------------------------------

fn cmd_validate(path: &std::path::Path) -> anyhow::Result<()> {
    let config = ConfigFile::load(path)?;
    let errors = config.validate();

    if errors.is_empty() {
        let name = config.pipeline.name.as_deref().unwrap_or("<unnamed>");
        println!(
            "OK: {} ({name}): {} transform(s), source {} -> destination {}",
            path.display(),
            config.transform.len(),
            config.source.type_name(),
            config.destination.type_name(),
        );
        Ok(())
    } else {
        for error in &errors {
            eprintln!("error: {error}");
        }
        anyhow::bail!("configuration invalid: {} error(s)", errors.len());
    }
}

// ---------------------------------------------------------------------------
// plan (dry-run)
// ---------------------------------------------------------------------------

fn cmd_plan(path: &std::path::Path) -> anyhow::Result<()> {
    let config = ConfigFile::load(path)?;
    let errors = config.validate();
    if !errors.is_empty() {
        for error in &errors {
            eprintln!("error: {error}");
        }
        anyhow::bail!("configuration invalid: {} error(s)", errors.len());
    }

    let name = config.pipeline.name.as_deref().unwrap_or("<unnamed>");
    println!("Plan: {name}");
    println!(
        "Source: {} ({})",
        config.source.type_name(),
        config.source.url(),
    );
    println!(
        "Destination: {} ({})",
        config.destination.type_name(),
        config.destination.url(),
    );

    let source = open_source(&config)?;
    let destination = open_dest(&config)?;
    let transforms = build_transforms(&config)?;
    let transform_refs: Vec<&dyn Transformation> = transforms.iter().map(|b| &**b).collect();
    let syncer = Syncer::new(&source, &destination);
    let report = syncer.plan(&transform_refs)?;

    let policy = config.policy.unwrap_or_default();
    let policy_report = check(&policy, &report.diff, &report.output);

    if !policy_report.is_allowed() {
        for v in &policy_report.denied {
            eprintln!(
                "error: policy denied path `{}` (pattern: `{}`)",
                v.path.as_str(),
                v.pattern
            );
        }
        for v in &policy_report.review {
            eprintln!(
                "error: policy requires review for `{}` (pattern: `{}`)",
                v.path.as_str(),
                v.pattern
            );
        }
        if policy_report.deleted_count > policy.max_deleted_files.unwrap_or(u64::MAX) {
            eprintln!(
                "error: policy exceeded max_deleted_files: {} > {}",
                policy_report.deleted_count,
                policy.max_deleted_files.unwrap_or(0)
            );
        }
        anyhow::bail!(
            "policy check failed: {} violation(s)",
            policy_report.denied.len() + policy_report.review.len()
        );
    }

    println!(
        "Transforms: {}",
        transform_refs
            .iter()
            .map(|t| t.name())
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!(
        "Changes: {} added, {} removed, {} modified, {} renamed",
        report.diff.added.len(),
        report.diff.removed.len(),
        report.diff.modified.len(),
        report.diff.renamed.len(),
    );
    println!("Dry run: no commits were written.");
    Ok(())
}

// ---------------------------------------------------------------------------
// migrate (E2E)
// ---------------------------------------------------------------------------

fn cmd_migrate(
    path: &std::path::Path,
    state_path: &std::path::Path,
    history: bool,
) -> anyhow::Result<()> {
    let config = ConfigFile::load(path)?;
    let errors = config.validate();
    if !errors.is_empty() {
        for error in &errors {
            eprintln!("error: {error}");
        }
        anyhow::bail!("configuration invalid: {} error(s)", errors.len());
    }

    let name = config.pipeline.name.as_deref().unwrap_or("<unnamed>");
    println!("Migrate: {name}");

    let source = open_source(&config)?;
    let source_head = source.head_commit_id()?;

    let Some(source_head) = source_head else {
        println!("No source commits: nothing to migrate.");
        return Ok(());
    };

    let state = reposync_state::State::open_for_pipeline(state_path, pipeline_id(&config)?)
        .with_context(|| format!("failed to open state database at {}", state_path.display()))?;

    let transforms = build_transforms(&config)?;
    let transform_refs: Vec<&dyn Transformation> = transforms.iter().map(|b| &**b).collect();

    if history {
        return cmd_migrate_history(&config, &source, &state, &transform_refs, name);
    }

    // --- Snapshot mode (default): one commit at HEAD. ---

    let dest = open_dest(&config)?;

    // Incremental guard: skip source heads we have already exported.
    if state.has_source(&source_head)? {
        println!(
            "Already synced source commit {}. No changes made.",
            source_head.as_str()
        );
        return Ok(());
    }

    let plan = Syncer::new(&source, &dest).plan(&transform_refs)?;

    let policy = config.policy.unwrap_or_default();
    let policy_report = check(&policy, &plan.diff, &plan.output);
    if !policy_report.is_allowed() {
        for v in &policy_report.denied {
            eprintln!(
                "error: policy denied path `{}` (pattern: `{}`)",
                v.path.as_str(),
                v.pattern
            );
        }
        anyhow::bail!("policy check failed");
    }

    let push_url = match &config.destination {
        Destination::Git { url, .. } => Some(url.as_str()),
    };
    let commit = CommitSpec::new(format!("migrate: {name}"));
    let result = Syncer::new(&source, &dest).run(&transform_refs, &commit, push_url)?;

    if let Some(destination_id) = &result.commit_id {
        state.record(&source_head, destination_id)?;
    }

    if result.changed {
        println!(
            "Done: {} added, {} removed, {} modified, {} renamed",
            result.diff.added.len(),
            result.diff.removed.len(),
            result.diff.modified.len(),
            result.diff.renamed.len(),
        );
        println!("Commit: {}", result.commit_id.as_ref().unwrap());
        if result.pushed {
            println!("Pushed to {}", config.destination.url());
        }
        println!(
            "Recorded mapping: {} -> {}",
            source_head.as_str(),
            result.commit_id.as_ref().unwrap()
        );
    } else {
        println!("No changes to apply: destination already up to date.");
    }
    Ok(())
}

/// History mode: replay every source commit through the transform DAG.
fn cmd_migrate_history(
    config: &ConfigFile,
    source: &GitRepo,
    state: &reposync_state::State,
    transform_refs: &[&dyn Transformation],
    name: &str,
) -> anyhow::Result<()> {
    let dest = open_dest(config)?;

    // Policy gate on the final (HEAD) output, equivalent to snapshot mode.
    let plan = Syncer::new(source, &dest).plan(transform_refs)?;
    let policy = config.policy.clone().unwrap_or_default();
    let policy_report = check(&policy, &plan.diff, &plan.output);
    if !policy_report.is_allowed() {
        for v in &policy_report.denied {
            eprintln!(
                "error: policy denied path `{}` (pattern: `{}`)",
                v.path.as_str(),
                v.pattern
            );
        }
        anyhow::bail!("policy check failed");
    }

    let (author_map, message_prefix) = history_transforms(config);
    let report = reposync_migration::replay_history(
        source,
        &dest,
        transform_refs,
        Some(state),
        author_map.as_ref(),
        message_prefix.as_deref(),
    )?;

    if report.commits_replayed == 0 && report.commits_skipped > 0 {
        println!(
            "Already synced: {} source commit(s) already mapped. No changes made.",
            report.commits_skipped
        );
        return Ok(());
    }

    let push_url = match &config.destination {
        Destination::Git { url, .. } => Some(url.as_str()),
    };
    if let Some(url) = push_url {
        dest.push(url)?;
    }

    println!(
        "History migrated: {} commit(s) replayed, {} skipped.",
        report.commits_replayed, report.commits_skipped
    );
    if let Some(last) = &report.last_commit {
        println!("Last commit: {last}");
    }
    if push_url.is_some() {
        println!("Pushed to {}", config.destination.url());
    }
    println!("Migrate: {name} (history) complete.");
    Ok(())
}

// ---------------------------------------------------------------------------
// diff
// ---------------------------------------------------------------------------

fn cmd_diff(path: &std::path::Path) -> anyhow::Result<()> {
    let config = ConfigFile::load(path)?;
    let errors = config.validate();
    if !errors.is_empty() {
        for error in &errors {
            eprintln!("error: {error}");
        }
        anyhow::bail!("configuration invalid: {} error(s)", errors.len());
    }

    let source = open_source(&config)?;
    let dest = open_dest(&config)?;
    let source_snapshot = source.head_snapshot()?;
    let dest_snapshot = dest.head_snapshot()?;
    let diff = diff(&source_snapshot, &dest_snapshot);

    println!(
        "Diff: {} added, {} removed, {} modified, {} renamed",
        diff.added.len(),
        diff.removed.len(),
        diff.modified.len(),
        diff.renamed.len(),
    );

    for entry in &diff.added {
        println!("+ {}", entry.path.as_str());
    }
    for entry in &diff.removed {
        println!("- {}", entry.path.as_str());
    }
    for entry in &diff.modified {
        println!("~ {}", entry.path.as_str());
    }
    for entry in &diff.renamed {
        println!("r {} -> {}", entry.from.as_str(), entry.to.as_str());
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// sync (bidirectional, loop-preventing)
// ---------------------------------------------------------------------------

fn cmd_sync(path: &std::path::Path, state_path: &std::path::Path) -> anyhow::Result<()> {
    let config = ConfigFile::load(path)?;
    let errors = config.validate();
    if !errors.is_empty() {
        for error in &errors {
            eprintln!("error: {error}");
        }
        anyhow::bail!("configuration invalid: {} error(s)", errors.len());
    }

    let name = config.pipeline.name.as_deref().unwrap_or("<unnamed>");
    println!("Sync: {name}");

    let internal = open_source(&config)?;
    let public = open_dest(&config)?;
    let transforms = build_transforms(&config)?;
    let transform_refs: Vec<&dyn Transformation> = transforms.iter().map(|b| &**b).collect();
    let state = reposync_state::State::open_for_pipeline(state_path, pipeline_id(&config)?)
        .with_context(|| format!("failed to open state database at {}", state_path.display()))?;

    match reposync_migration::sync(
        &internal,
        &public,
        &transform_refs,
        &state,
        SyncStrategy::Fail,
    ) {
        Ok(report) => {
            let internal_url = match &config.source {
                Source::Git { url, .. } => url.clone(),
            };
            let public_url = match &config.destination {
                Destination::Git { url, .. } => url.clone(),
            };
            // Only push a side when that side actually received new commits.
            if report.imported > 0 {
                internal.push(&internal_url)?;
            }
            if report.forward_replayed > 0 {
                public.push(&public_url)?;
            }
            println!(
                "Sync complete: {} forward, {} generated skipped, {} imported, {} conflict(s)",
                report.forward_replayed,
                report.skipped_generated,
                report.imported,
                report.conflicts.len()
            );
            Ok(())
        }
        Err(MigrationError::Conflict(report)) => {
            for conflict in &report.conflicts {
                eprintln!(
                    "error: sync conflict on public commit {}: {}",
                    conflict.public_commit, conflict.message
                );
            }
            anyhow::bail!(
                "sync aborted: {} conflict(s) detected",
                report.conflicts.len()
            );
        }
        Err(error) => Err(error.into()),
    }
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn open_source(config: &ConfigFile) -> anyhow::Result<GitRepo> {
    // For the M7 MVP we work with local paths or git URLs. If the source is a
    // URL we clone it into a temp dir; otherwise we open the existing path.
    match &config.source {
        Source::Git { url, ref_name } => {
            let repo = GitRepo::open_or_clone(url, temp_source_path())
                .with_context(|| format!("failed to open source repository at {url}"))?;
            repo.checkout_ref(ref_name)
                .with_context(|| format!("failed to check out source ref `{ref_name}` in {url}"))?;
            Ok(repo)
        }
    }
}

fn pipeline_id(config: &ConfigFile) -> anyhow::Result<String> {
    let bytes = serde_json::to_vec(config).context("failed to serialize pipeline identity")?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn open_dest(config: &ConfigFile) -> anyhow::Result<GitRepo> {
    match &config.destination {
        Destination::Git { url, branch } => {
            let repo = GitRepo::clone(url, temp_dest_path())
                .with_context(|| format!("failed to clone destination repository at {url}"))?;
            repo.checkout_ref(branch).with_context(|| {
                format!("failed to check out destination branch `{branch}` in {url}")
            })?;
            Ok(repo)
        }
    }
}

/// Use a per-run temp dir so each command invocation is isolated.
fn temp_source_path() -> std::path::PathBuf {
    unique_temp_path("reposync-source")
}

fn temp_dest_path() -> std::path::PathBuf {
    unique_temp_path("reposync-dest")
}

fn unique_temp_path(prefix: &str) -> std::path::PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    std::env::temp_dir().join(format!("{prefix}-{}-{nonce}", std::process::id()))
}

/// Extract the history-only transforms (`author_mapping`, `commit_message`)
/// from the pipeline. These are applied during history replay, not as snapshot
/// transforms, so they are collected separately and passed to `replay_history`.
fn history_transforms(config: &ConfigFile) -> (Option<BTreeMap<String, String>>, Option<String>) {
    let mut author_map = None;
    let mut message_prefix = None;
    for node in &config.transform {
        if let Some(args) = &node.author_mapping {
            author_map = Some(args.mapping.clone());
        }
        if let Some(args) = &node.commit_message {
            message_prefix = Some(args.message.clone());
        }
    }
    (author_map, message_prefix)
}

fn build_transforms(config: &ConfigFile) -> anyhow::Result<Vec<Box<dyn Transformation>>> {
    let mut transforms: Vec<Box<dyn Transformation>> = Vec::new();
    for node in &config.transform {
        let Some(kind) = node.kind() else { continue };
        let boxed: Box<dyn Transformation> = match kind {
            TransformKind::Filter => {
                let paths = node.filter.as_ref().unwrap().paths.clone();
                Box::new(Filter::new(paths))
            }
            TransformKind::Delete => {
                let paths = node.delete.as_ref().unwrap().paths.clone();
                Box::new(Delete::new(paths))
            }
            TransformKind::Copy => {
                let args = node.copy.as_ref().unwrap();
                Box::new(Copy::new(&args.from, &args.to))
            }
            TransformKind::Move => {
                let args = node.r#move.as_ref().unwrap();
                Box::new(Move::new(&args.from, &args.to))
            }
            TransformKind::Rename => {
                let args = node.rename.as_ref().unwrap();
                Box::new(Rename::new(&args.from, &args.to))
            }
            TransformKind::Replace => {
                let args = node.replace.as_ref().unwrap();
                Box::new(Replace::new(args.file.clone(), args.replacements.clone()))
            }
            TransformKind::StripPrefix => {
                let args = node.strip_prefix.as_ref().unwrap();
                Box::new(StripPrefix::new(args.path.clone()))
            }
            TransformKind::DependencyRewrite => {
                let args = node.dependency_rewrite.as_ref().unwrap();
                Box::new(DependencyRewrite::new(
                    args.package_rename.clone(),
                    args.workspace_version.clone(),
                ))
            }
            TransformKind::ImportRewrite => {
                let args = node.import_rewrite.as_ref().unwrap();
                Box::new(ImportRewrite::new(args.renames.clone()))
            }
            TransformKind::RegexReplace => {
                let args = node.regex_replace.as_ref().unwrap();
                Box::new(RegexReplace::new(
                    args.files.clone(),
                    args.pattern.clone(),
                    args.replacement.clone(),
                ))
            }
            TransformKind::Prepend => {
                let args = node.prepend.as_ref().unwrap();
                Box::new(Prepend::new(args.files.clone(), args.content.clone()))
            }
            TransformKind::Append => {
                let args = node.append.as_ref().unwrap();
                Box::new(Append::new(args.files.clone(), args.content.clone()))
            }
            TransformKind::Patch => {
                let args = node.patch.as_ref().unwrap();
                Box::new(Patch::new(args.file.clone(), args.patch.clone()))
            }
            TransformKind::Metadata => {
                let args = node.metadata.as_ref().unwrap();
                Box::new(Metadata::new(args.key.clone(), args.value.clone()))
            }
            TransformKind::Custom => {
                let args = node.custom.as_ref().unwrap();
                let plugin = reposync_transform::PluginTransform::from_file(
                    &args.name,
                    std::path::Path::new(&args.path),
                    &args.args,
                )
                .map_err(anyhow::Error::new)?;
                Box::new(plugin)
            }
            // Author/commit rewrites are applied during history replay, not the
            // snapshot pipeline, so they produce no snapshot transform here.
            TransformKind::AuthorMapping | TransformKind::CommitMessage => continue,
        };
        transforms.push(boxed);
    }
    Ok(transforms)
}

// ---------------------------------------------------------------------------
// path — package source materialization
// ---------------------------------------------------------------------------

#[allow(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
fn cmd_path(
    spec: Option<String>,
    registry: Option<String>,
    version: Option<String>,
    json: bool,
    verbose: u8,
    evict: bool,
    all_versions: bool,
    all: bool,
) -> anyhow::Result<()> {
    // Handle --evict.
    if evict || all_versions || all {
        if all {
            let count =
                reposync_package::evict_all().unwrap_or_else(|e| exit_with_package_error(&e));
            if verbose > 0 {
                eprintln!("reposync: evicted {count} package(s)");
            }
            if json {
                println!("{{\"evicted\":{count}}}");
            }
            return Ok(());
        }
        let Some(spec) = spec else {
            eprintln!("error: package spec required for --evict (or use --all)");
            std::process::exit(2);
        };
        // Build spec for eviction.
        let spec_str = if let Some(reg) = registry.clone() {
            if spec.contains(':') {
                spec.clone()
            } else {
                format!("{reg}:{spec}")
            }
        } else {
            spec.clone()
        };
        let count = reposync_package::evict_spec(
            &spec_str,
            version.as_deref(),
            all_versions,
            false,
        )
        .unwrap_or_else(|e| exit_with_package_error(&e));
        if verbose > 0 {
            eprintln!("reposync: evicted {count} package(s) for {spec_str}");
        } else if json {
            println!("{{\"evicted\":{count}}}");
            return Ok(());
        }
        return Ok(());
    }

    let Some(spec) = spec else {
        eprintln!("error: path takes exactly one package spec");
        std::process::exit(2);
    };

    // Validate single spec.
    if spec.trim().is_empty() {
        eprintln!("error: package spec must not be empty");
        std::process::exit(2);
    }

    // Registry override handling: if --registry is given and spec has no prefix,
    // prefix it.
    let effective_spec = if let Some(reg) = registry {
        if spec.contains(':') {
            // Spec already has registry; ensure they agree or honor spec.
            spec.to_owned()
        } else {
            format!("{reg}:{spec}")
        }
    } else {
        spec.to_owned()
    };

    let verbose_flag = verbose > 0;
    let result = reposync_package::resolve_and_materialize(
        &effective_spec,
        version.as_deref(),
        verbose_flag,
    )
    .unwrap_or_else(|e| exit_with_package_error(&e));

    // Check for stdout contract: exactly one line absolute path.
    let path_str = result.display().to_string();

    if json {
        // Need version and registry for JSON. Re-parse spec to extract.
        let parsed = reposync_package::PackageSpec::parse(&effective_spec, version.as_deref())
            .unwrap_or_else(|e| exit_with_package_error(&e));
        // Determine actual version from path (last component before /package).
        let version_from_path = result
            .parent()
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| parsed.version.clone().unwrap_or_else(|| "unknown".to_owned()));
        let cached = true; // resolve_and_materialize always caches; we could check is_cached but it is true.
        let obj = serde_json::json!({
            "registry": parsed.registry.as_str(),
            "name": parsed.name,
            "version": version_from_path,
            "path": path_str,
            "cached": cached
        });
        println!("{}", obj);
    } else {
        println!("{}", path_str);
    }

    Ok(())
}

fn exit_with_package_error(e: &reposync_package::PackageError) -> ! {
    let code = e.exit_code();
    // Map to required stderr messages per PRODUCT.md#10.
    // PackageError Display already contains the required wording.
    eprintln!("error: {e}");
    if matches!(
        e,
        reposync_package::PackageError::UnsupportedRegistry { .. }
    ) {
        eprintln!("{}", reposync_package::PackageError::supported_registries());
    }
    std::process::exit(code);
}
