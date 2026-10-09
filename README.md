# RepoSync

RepoSync is a Git-aware repository migration and synchronization engine for
monorepos. It reads a source repository into an in-memory snapshot, applies an
ordered transformation pipeline, checks the resulting diff against a
destination repository, and optionally commits and pushes the result.

The project is written in Rust and is designed for deterministic, testable,
dry-run-safe repository transformations.

## Features

- Snapshot-based repository processing; transforms do not modify a working tree.
- Git source and destination repositories using local paths, SSH, HTTPS, and
  other Git-supported locations.
- Explicit source refs and destination branches.
- Snapshot migration and history replay modes.
- Incremental state tracking in SQLite.
- Bidirectional sync with generated-commit markers and conflict detection.
- Deterministic path/content diffs with rename detection.
- Policy checks for denied paths, required review paths, and deletion limits.
- Built-in transforms for filtering, deletion, copying, moving, renaming,
  replacement, prefix stripping, metadata, patches, dependency rewriting, and
  import rewriting.
- WASM custom transformation plugins.
- Package source materialization: resolve npm/pypi packages to local cached directories.

## Quick start

Build and test the workspace:

```bash
cargo build --workspace
cargo test --workspace
```

Validate a pipeline:

```bash
cargo run -p reposync-cli -- validate examples/ok.yaml
```

Preview a migration without writing a commit:

```bash
cargo run -p reposync-cli -- plan path/to/pipeline.yaml
```

Run a snapshot migration:

```bash
cargo run -p reposync-cli -- migrate path/to/pipeline.yaml
```

Replay source history instead of creating one snapshot commit:

```bash
cargo run -p reposync-cli -- migrate path/to/pipeline.yaml --history
```

Other commands:

```text
reposync init                Scaffold a new pipeline configuration file
reposync validate <config>   Validate a pipeline configuration
reposync plan <config>       Show a dry-run plan
reposync migrate <config>    Transform, commit, and push a repository
reposync diff <config>       Show a path/content diff
reposync sync <config>       Run bidirectional synchronization
reposync path <spec>         Resolve a package spec to its unpacked source directory
```

### Package Source Access

`reposync path` resolves npm and pypi package specs to local cached directories:

```bash
# Resolve latest version (caches locally)
cargo run -p reposync-cli -- path zod

# Resolve specific version
cargo run -p reposync-cli -- path zod@3.22.4

# PyPI packages
cargo run -p reposync-cli -- path pypi:requests

# JSON output with metadata
cargo run -p reposync-cli -- path --json zod

# Evict from cache
cargo run -p reposync-cli -- path --evict zod
```

Packages are cached under `~/.cache/reposync/packages/` and can be used offline
once cached. The command is shell-composable for use with tools like `rg`:
```bash
rg "parse" $(cargo run -p reposync-cli -- path zod)
```

## Configuration

Pipeline files use YAML. A minimal migration looks like this:

```yaml
pipeline:
  name: public-sdk

source:
  type: git
  url: /path/to/internal-repository
  ref: main

transform:
  - filter:
      paths:
        - packages/sdk/**
        - LICENSE
  - move:
      from: packages/sdk
      to: .
  - delete:
      paths:
        - "**/.internal"
        - "**/secrets/**"

destination:
  type: git
  url: /path/to/public-repository
  branch: main

policy:
  deny:
    - .env
    - "**/credentials/**"
  max_deleted_files: 100
```

Transforms run from top to bottom. Each list item must contain exactly one
transform. Glob patterns support `*`, `?`, and `**` path matching.

See the complete examples in [`examples/`](examples/), including string
rewrites, metadata, patches, and monorepo extraction.

### Creating a Configuration

Use `reposync init` to scaffold a new configuration file:

```bash
# Create reposync.yaml in the current directory
cargo run -p reposync-cli -- init

# Create a configuration file at a custom path
cargo run -p reposync-cli -- init --output my-migration.yaml
```

The generated configuration includes:
- Common transforms (filter, move, delete)
- Safety policy with denied paths
- Comments explaining each section
- Next steps for validation and migration

## State and safety

`migrate` stores source-to-destination commit mappings in `.sync/state.db` by
default. Pass `--state <path>` to use another database. Re-running a migration
for an already mapped source commit is a no-op.

`plan` performs the same source read and transformation pipeline as `migrate`,
but does not create or push commits. Policy checks run before migration writes
to the destination.

## Workspace layout

| Crate | Responsibility |
| --- | --- |
| `core` | Repository snapshots, paths, blobs, commits, and metadata |
| `git` | Git repository access and commit/tree operations |
| `config` | YAML schema and semantic validation |
| `transform` | Ordered transformation engine and built-ins |
| `filesystem` | Snapshot/worktree ingest and materialization |
| `diff` | Deterministic snapshot diff and rename detection |
| `policy` | Safety checks before writes |
| `state` | SQLite commit mappings |
| `migration` | History replay and bidirectional sync |
| `sync` | Snapshot planning and commit/push orchestration |
| `package` | Registry package source materialization (npm/pypi) |
| `cli` | `reposync` command-line interface |

## Development

Format and lint the workspace before submitting changes:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

The design and milestone plan are documented in [`PLAN.md`](PLAN.md).

## License

RepoSync is licensed under either the Apache License, Version 2.0 or the MIT
License, at your option.
