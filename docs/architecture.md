# Architecture

RepoSync separates repository access, transformation, policy, and write
operations so a plan can run without mutating the destination.

```text
Git source
    │
    ▼
RepositorySnapshot ──► ordered transforms ──► transformed snapshot
                                               │
                         destination snapshot ─┘
                                   │
                                   ▼
                              SnapshotDiff
                                   │
                           policy check
                                   │
                         commit and optional push
```

## Core model

`RepositorySnapshot` stores tracked files in a deterministic `BTreeMap` keyed
by validated `RepoPath` values. File contents are represented by content-
addressed blobs and preserve regular, executable, and symlink modes.

The filesystem crate converts between snapshots and working directories. The
Git crate reads trees directly and writes new trees/commits without requiring
transforms to operate on a working tree.

## Planning and migration

`Syncer::plan` reads the source `HEAD`, runs the transform runner, and computes
the diff against the destination `HEAD`. `Syncer::run` commits only when the
diff is non-empty, then optionally pushes the destination remote.

Snapshot migration creates one destination commit for the selected source
revision. History migration walks source commits oldest-first, transforms each
snapshot, preserves parent relationships, and records mappings in state.

Bidirectional sync uses generated-commit markers to recognize commits produced
by RepoSync. External destination changes are reported as conflicts under the
default fail strategy.

## Crate responsibilities

- `core`: paths, blobs, snapshots, commits, and signatures.
- `git`: libgit2 repository operations, tree serialization, and markers.
- `config`: YAML parsing and semantic validation.
- `transform`: ordered transformation runner and built-ins.
- `diff`: deterministic additions, removals, modifications, and renames.
- `policy`: pre-write safety checks.
- `state`: SQLite source/destination commit mappings.
- `migration`: history replay and two-way synchronization.
- `sync`: snapshot planning, commit creation, and push orchestration.
- `cli`: the `reposync` binary.
