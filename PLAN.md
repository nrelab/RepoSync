# RepoSync — Build Plan

A programmable Git repository migration, synchronization, filtering, and
transformation engine. Design doc condensed into an executable build plan.

> Core identity: **"Git-aware programmable repository transformation and
> synchronization engine for monorepos."**
> Monorepo extraction/export is a first-class feature, not a Copybara clone.

---

## 0. Guiding Decisions

These lock in the architecture before any code is written.

| Decision | Choice | Rationale |
|---|---|---|
| Language | Rust for all core | Git, transform, diff, commit, push, concurrency, determinism. TS only for SDK/VS Code later |
| Git backend | `git2` (libgit2) for Phase 1 | Battle-tested for clone/commit/push. `GitRepository` trait isolates it so `gix` (gitoxide) can replace it later |
| Repo model | In-memory `RepositorySnapshot`, never the working tree | Deterministic transforms, dry-run, rollback, caching, easy testing |
| Transform core | `Transformation` trait + ordered DAG | Composition over hardcoded pipeline |
| Config | Declarative YAML DSL (`serde_yaml`/`serde_yml`) | Schema-validated before any repo work happens |
| State | `state.json` → SQLite (`rusqlite`) in Phase 2 | Commit mapping, incremental runs, sync loop prevention |
| Determinism | `BTreeMap` paths, fixed sort order, no `HashMap` iteration, fixed chunking | Reproducible diffs/commits across runs and machines |
| Plugins | WASM boundary | Sandboxing, language independence, portable |
| Errors | `thiserror` types + `anyhow` at edges | Structured errors in CLI, not string soup |

---

## 1. Milestones

Dependencies flow top to bottom. Phases 0–2 = the "wow" MVP
(`reposync migrate config.yaml`), which is the first releasable thing.

```text
M0 Scaffold ──► M1 Snapshot ──► M2 Config ──► M3 Git ──► M4 Transforms
                                                              │
                                        M5 Diff ──► M6 Commit/Push
                                                              │
                                              M7 CLI + Dry-run ◄── M8 Policy
                                                              │
                                                        M9 Test Harness
```

```text
                        ┌──────── Phase 2 (production) ────────┐
M10 State DB ──► M11 History Migration ──► M12 Bidirectional Sync
                                        │
                                        ├──► M13 AST + Dep Rewriting
                                        └──► M14 WASM Plugins
```

---

## 2. Milestone Detail

Each milestone has: **Goal**, **Tasks** (concrete), **Definition of Done**
(explicit verification), **Est**.

### M0 — Workspace Scaffold

**Goal:** Rust workspace that compiles clean and has the crate skeleton from the
design doc.

**Tasks**
- `cargo new` workspace with crates:
  `core`, `git`, `config`, `transform`, `filesystem`, `metadata`, `diff`,
  `migration`, `cli`.
- Empty `transformations/{builtin,examples}`, `tests/fixtures/`, `docs/`,
  `examples/` dirs.
- CI lint config (clippy, rustfmt), `.gitignore`.
- `clap`-based `reposync` bin printing `--version`.

**DoD**
- `cargo build --workspace` and `cargo clippy --workspace -- -D warnings` clean.
- `reposync --version` prints version string.

**Est:** 0.5d

---

### M1 — Repository Snapshot (the heart)

**Goal:** In-memory repository model. Everything else operates on this.

**Tasks**
- `RepoPath` newtype: validated, `/`-separated, `.`/`..` rejected, no leading `/`.
- `Blob` (content bytes + hash), `FileMode`.
- `FileEntry { path, content, mode }`.
- `Commit { id, parents, message, author, author_time, committer, ... }`.
- `RepositorySnapshot { files: BTreeMap<RepoPath, FileEntry>, metadata }`.
- Serialization for snapshots (JSON; enables dry-run, caching, tests).
- `filesystem` crate: materialize snapshot → working dir and ingest dir →
  snapshot (lossless for tracked files).

**DoD**
- Round-trip test: dir → snapshot → dir produces identical tree.
- Deterministic ordering asserted in unit tests.

**Est:** 2d

---

### M2 — Config Schema + Validation

**Goal:** Parse and validate the YAML DSL before touching any repo.

**Tasks**
- Serde structs mirroring the DSL: `Pipeline { name, source, transform, destination }`.
- `source`/`destination` providers: `git { url, ref, branch }`, with local/SSH/HTTPS.
- Transform node enum (all known transforms, even not-yet-implemented ones fail validation with a clear message).
- `reposync validate <config>` command.
- Semantic validation: filter paths exist, move targets don't collide, destination != source, unknown keys rejected.

**DoD**
- `reposync validate examples/ok.yaml` passes; `examples/bad.yaml` fails with actionable errors.

**Est:** 2d

---

### M3 — Git Layer

**Goal:** `GitRepository` trait + `git2` implementation.

**Tasks**
- Trait: `clone`, `fetch`, `checkout`, `diff`, `commit`, `push` (from design §6).
- `git2` backend for: local dir, SSH remote, HTTPS remote.
- Working tree ↔ snapshot ingest/materialize via `filesystem` crate.
- Preserve identity for no-op runs (same snapshot → no commit).

**DoD**
- Clone a fixture repo, ingest snapshot, materialize, git status shows clean tree.
- Integration test: local source → local dest push.

**Est:** 3d

---

### M4 — Transformation Engine + Tier 1

**Goal:** `Transformation` trait and the composable DAG runtime.

**Tasks**
- `pub trait Transformation { fn name(&self); fn apply(&self, ctx: &mut TransformContext) -> Result<TransformResult>; }`
- `TransformContext` = `RepositorySnapshot` + run state (counts, warnings, events).
- `TransformResult` = files changed + structured event.
- Runner: ordered application with per-step timing and cancellation on error.
- Tier 1 transforms (§14): **filter, delete, copy, move, rename**.
  - `filter`: glob-aware path keep-set.
  - `delete`: glob-aware path remove-set (incl. `**/secrets/**` style).
  - `move` / `rename`: path rewrite with collision detection.
- Shared glob util (used by filter/delete/move), `BTreeMap`-safe path ops.
- Dry-run mode: run in-memory, never touch disk/git beyond source read.

**DoD**
- Unit tests per transform: input snapshot → expected snapshot + expected diff.
- Deterministic: same input, two runs → identical output snapshots.

**Est:** 4d

---

### M5 — Diff Engine

**Goal:** Path/content-level diff over snapshots.

**Tasks**
- `SnapshotDiff { added, modified, removed, renamed }` with content hashing.
- Rename detection (move/rename produce renames, not delete+add).
- Machine-readable diff structs (feed CLI + policy + sync later).

**DoD**
- Fixture: filter+move pipeline produces the exact expected diff.

**Est:** 2d

---

### M6 — Commit + Push

**Goal:** Write transformed snapshot back to destination repo.

**Tasks**
- Commit generation from `CommitSpec` (message, author identity, timestamps).
- Push to local/SSH/HTTPS destination.
- Author/committer mapping hook (used later for history mode).
- Atomicity: build commit fully in memory; only write tree/commit on success.

**DoD**
- E2E: local source → transforms → local dest has exactly expected files.
- No-op run makes zero commits.

**Est:** 2d

---

### M7 — CLI + Dry Run

**Goal:** First usable workflows with dry-run from day one.

**Tasks**
- `reposync init` (scaffold config), `plan` (dry-run + report), `migrate`, `diff`.
- Dry-run report (§11): source/dest, transformation list with ✓, change summary
  (added/modified/deleted counts), "No changes pushed."
- `--dry-run` on every mutating command. `plan` = `migrate --dry-run`.
- Rich but dependency-light terminal output (plain ANSI, no tui yet).

**DoD**
- `reposync plan ./copy.yaml` prints full report and mutates nothing.

**Est:** 2d

---

### M8 — Policy Engine / Safety Model

**Goal:** First-class safety gate before push.

**Tasks**
- Policy config: `deny` paths (glob), `require_review` files, `max_deleted_files`.
- Diff → policy check → approval → push (design §10).
- Denied path or over-limit deletes → hard failure with report.
- Secret detection hook point (scan for `*.pem`, `.env`, `**/credentials/**`).

**DoD**
- Fixture with a `.env` file: `plan` and `migrate` refuse with clear output.

**Est:** 2d

---

### M9 — Test Harness + Fixtures

**Goal:** Reproducible repo-to-repo tests for the whole stack.

**Tasks**
- Fixture layout: `tests/fixtures/{source,expected,destination}`.
- Harness: build temp source repo from fixture → run pipeline → diff against
  expected repo.
- `git` CLI (or `git2`) to construct fixture repos in tests.
- Golden tests for every transform: input → expected output + expected diff.
- Property: determinism (run twice, byte-identical commits).

**DoD**
- `cargo test` green; every transform has a golden fixture.

**Est:** 2d

---

### M10 — State DB + Change Mapping

**Goal:** Know which destination commit corresponds to which source commit.

**Tasks**
- `.sync/state.json` MVP, then `rusqlite` `state.db`.
- `commit_mapping(source_commit PK, destination_commit, timestamp)`.
- Persist mapping on every migrate/sync.
- Incremental execution: skip source commits already mapped.

**DoD**
- Run migrate twice; second run is a no-op.
- Mapping table has exactly one row per migrated commit.

**Est:** 2d

---

### M11 — History Migration

**Goal:** Transform commits individually (design §7).

**Tasks**
- Snapshot-mode stays default; add `--history`.
- Commit traversal (topological, oldest-first), replay through the transform DAG.
- Author mapping + commit-message transformation.
- Preserve ordering; remap parents to transformed parents.
- History verification: transformed tree == snapshot-mode result for same head.

**DoD**
- `A–B–C–D` fixture → `A'–B'–C'–D'` with correct parent chain and authors.
- Verification test passes (tree at head matches snapshot-mode output).

**Est:** 5d

---

### M12 — Bidirectional Sync

**Goal:** Two-way sync with loop prevention (design §9).

**Tasks**
- One-way `sync` (internal → public) building on commit mapping.
- Two-way sync: detect destination-side changes, map back through
  `commit_mapping` + `generated_commit` marker.
- Loop prevention: recognize generated commits, refuse to re-import them.
- Conflict detection + explicit merge strategy config (default: fail with report).

**DoD**
- Integration test: internal A → public X → public Y → sync back → recognizes
  X/Y as generated, no loop, no duplicate.

**Est:** 5d

---

### M13 — AST-Aware Transforms + Dependency Rewriting

**Goal:** The monorepo-extraction differentiator (design §16–17).

**Tasks**
- `tree-sitter` integration for TS first; JSON/YAML/TOML via serde; later
  Rust/Python/Markdown.
- `replace` upgrade path: raw string → AST-aware module path rewrite
  (`@internal/foo` → `@public/foo`).
- Dependency rewriting: `workspace:*` → pinned version + package rename map.
- `metadata` + `commit_message` + `author_mapping` transforms (Tier 3).

**DoD**
- Fixture: monorepo → filter two packages → strip prefix → dest two public
  repos with rewritten imports and dependency versions.

**Est:** 6d

---

### M14 — Plugin Ecosystem (WASM)

**Goal:** Custom transformations via a sandboxed boundary.

**Tasks**
- WASM plugin ABI (`wit` interface): `transform(ctx) -> Result<TransformResult>`.
- Host: `wasmtime` runner, capabilities (snapshot access, path ops, no network).
- Plugin registry + `custom` transform in config.
- Determinism + sandbox verification in tests.

**DoD**
- A plugin written in (e.g.) Go/TS compiled to WASM runs in a pipeline.

**Est:** 5d

---

## 3. Phase 1 Task Sequence (first commits)

Recommended commit order for the MVP:

1. Workspace scaffold + CI lint
2. `RepoPath`, `Blob`, `FileEntry`, `Commit`, `RepositorySnapshot` + serde
3. `filesystem` ingest/materialize + round-trip test
4. Config structs + `validate` command
5. `GitRepository` trait + `git2` backend + local clone/push test
6. `Transformation` trait + runner + **filter**
7. **delete, copy, move, rename** + unit tests
8. Diff engine
9. Commit/push wiring → **`reposync migrate` works end-to-end**
10. Dry-run report + `plan` command
11. Policy engine + deny/limit gates
12. Test harness + golden fixtures for all transforms

---

## 4. Risk Register

| Risk | Mitigation |
|---|---|
| History mode scope creep | Snapshot mode is the shipped MVP; history is a separate milestone behind a flag |
| libgit2 limitations (shallow, weird repos) | `GitRepository` trait isolates backend; swap to `gix` later |
| WASM plugin perf/sandbox surprises | Plugins last; built-in Rust transforms cover 95% of needs |
| Determinism regressions | Golden + double-run tests in CI from M9 onward |
| Sync loops | Loop prevention is a hard requirement of M12, tested before merge |
| Regex-based replace corrupting code | AST-aware path (M13) is the real answer; regex is interim |

---

## 5. Out of Scope for v1

- Python core (never — TS SDK/VS Code only later)
- Web UI / visualization
- GitHub/GitLab/Gerrit providers (trait hooks exist, providers later)
- Secret scanning engine (detection hook only)
- Partial migrations, CI-driven dashboards
