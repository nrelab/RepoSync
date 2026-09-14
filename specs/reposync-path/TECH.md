# TECH.md — `reposync path` — Package Source Access

## Context

`reposync path <spec>` extends RepoSync from Git-repository mirroring to registry-package source materialization, giving coding agents a shell-composable filesystem path for any third-party package. See `PRODUCT.md` for user-visible behavior (registry routing, cache layout, stdout contract, error codes).

**Current system.** RepoSync is a Rust workspace (`resolver = "2"`) with 11 crates plus a harness, pinned at `4f9140588d9bc7cb3b07e3bafa82214fa4e65bd1` on `https://github.com/nrelab/RepoSync`:

- CLI entry is [`crates/cli/src/main.rs:19 @ 4f91405`](https://github.com/nrelab/RepoSync/blob/4f9140588d9bc7cb3b07e3bafa82214fa4e65bd1/crates/cli/src/main.rs#L19) — `Cli`/`Command` via `clap::Parser`/`Subcommand` with `Validate/Plan/Migrate/Diff/Sync` variants that dispatch to `cmd_*` handlers. No package command exists yet; flags follow `--verbose`/`--json` conventions in existing commands.
- Workspace definition and lint policy at [`Cargo.toml:1 @ 4f91405`](https://github.com/nrelab/RepoSync/blob/4f9140588d9bc7cb3b07e3bafa82214fa4e65bd1/Cargo.toml#L1) — `unsafe_code = forbid`, `clippy::all/pedantic = warn`.
- State/cache today is SQLite-backed in [`crates/state/src/lib.rs:1 @ 4f91405`](https://github.com/nrelab/RepoSync/blob/4f9140588d9bc7cb3b07e3bafa82214fa4e65bd1/crates/state/src/lib.rs#L1) for commit mappings; no package cache exists. The cache root convention (`$REPOSYNC_CACHE` → `$XDG_CACHE_HOME` → `~/.cache`) is not yet implemented and must be introduced.
- Core types at [`crates/core/src/snapshot.rs:41 @ 4f91405`](https://github.com/nrelab/RepoSync/blob/4f9140588d9bc7cb3b07e3bafa82214fa4e65bd1/crates/core/src/snapshot.rs#L41) (`RepositorySnapshot`/`RepositoryMetadata`) and path handling at [`crates/core/src/path.rs:1 @ 4f91405`](https://github.com/nrelab/RepoSync/blob/4f9140588d9bc7cb3b07e3bafa82214fa4e65bd1/crates/core/src/path.rs#L1) are not reused for packages, but the `RepoPath` normalization lessons apply to `<registry>/<name>/<version>` keys.
- CI is [` .github/workflows/ci.yml:1 @ 4f91405`](https://github.com/nrelab/RepoSync/blob/4f9140588d9bc7cb3b07e3bafa82214fa4e65bd1/.github/workflows/ci.yml#L1) running `cargo test --workspace` and `cargo clippy --workspace`.

## Proposed changes

### 1. New crate `crates/package` (registry abstraction + cache)

Create `crates/package` to keep registry I/O, integrity, and cache concerns out of `crates/cli`. This mirrors the existing split (`core` types, `git` I/O, `state` persistence).

**Types:**
```rust
pub enum Registry { Npm, Pypi }
pub struct PackageSpec { registry: Registry, name: String, version_req: Option<String> }
pub struct ResolvedPackage { spec: PackageSpec, version: String, tarball_url: String, integrity: Integrity }
pub enum Integrity { Npm { shasum: String, integrity: Option<String> }, Pypi { sha256: String } }
pub struct PackagePath { registry: Registry, name: String, version: String, path: PathBuf }
```

**Parsing (`src/spec.rs`):** `PackageSpec::parse(&str, Option<&str>)` implements `PRODUCT.md#1-2`: bare `zod` → `Npm`, `pypi:requests` → `Pypi`, inline `@`/`==` version extraction, `--version` override with mismatch error (exit 2). Normalization: npm lowercases, preserves `@scope/`; PyPI normalizes per PEP 503 (`[-_.]+` → `-`, lowercased).

**Registry trait (`src/registry.rs`):**
```rust
#[async_trait]
pub trait RegistryClient: Send + Sync {
    async fn resolve(&self, spec: &PackageSpec) -> Result<ResolvedPackage, PackageError>;
    async fn fetch(&self, resolved: &ResolvedPackage, dest: &Path) -> Result<(), PackageError>;
}
struct NpmClient { http: reqwest::Client, base: Url } // https://registry.npmjs.org
struct PypiClient { http: reqwest::Client, base: Url } // https://pypi.org/pypi
```
`resolve` fetches `GET /<name>` (npm) or `GET /pypi/<name>/json` (PyPI), selects `dist-tag.latest` or explicit version, extracts `dist.tarball`/`integrity`/`shasum` (npm) or `urls[].digests.sha256` (PyPI). Uses `reqwest` with `rustls-tls` (already implied by `git` crate's TLS) and 30s timeout, HTTPS-only (reject `http://` with `PackageError::UnsupportedScheme`).

**Cache (`src/cache.rs`):**
- `fn cache_root() -> PathBuf` implements `PRODUCT.md#4`: `$REPOSYNC_CACHE` → `$XDG_CACHE_HOME/reposync` → `~/.cache/reposync` via `dirs` crate.
- Layout `packages/<registry>/<normalized-name>/<version>/package/` plus `packages/<registry>/<normalized-name>/<version>/.complete` sentinel file. Atomicity: download to `tmp/<uuid>.tgz`, verify integrity (`sha1`/`sha256`/`ssri` via `sha2` + `base64`), unpack to `tmp/<uuid>.dir/`, then `std::fs::rename` into final location. Uses `fs2::File::try_lock_exclusive` on `<version>/.lock` to serialize concurrent racers (`PRODUCT.md#5`); losers poll for `.complete` (up to 60s).
- `fn evict(spec: &PackageSpec, all_versions: bool)` and `evict_all()` for `PRODUCT.md#11-12`.

**Extraction (`src/extract.rs`):** npm tarballs are `gzip+tar`; PyPI sdist is `tar.gz` or `zip`. Detect via `Content-Type`/`file` magic. Extraction sandbox (`PRODUCT.md#13`): reject absolute entries, `..` components, and symlinks escaping `package/` (resolve `entry.path()` and ensure `starts_with(dest)`). Set permissions `0o644` files / `0o755` dirs, never `+x` from tar.

**Errors (`src/error.rs`):** `PackageError` maps to CLI exit codes per `PRODUCT.md#10`: `NotFound`→3, `Network`→4, `Integrity`→5, `Io`→6, `UnsupportedRegistry`/`InvalidSpec`→2, with `#[error]` strings matching PRODUCT.md wording.

### 2. CLI wiring (`crates/cli`)

Extend [`crates/cli/src/main.rs:31 @ 4f91405`](https://github.com/nrelab/RepoSync/blob/4f9140588d9bc7cb3b07e3bafa82214fa4e65bd1/crates/cli/src/main.rs#L31):

```rust
enum Command {
    // existing ...
    Path {
        spec: String,
        #[arg(long, short = 'r')] registry: Option<String>,
        #[arg(long, short = 'v')] version: Option<String>,
        #[arg(long)] json: bool,
        #[arg(long, short)] verbose: bool,
        #[arg(long)] evict: bool,
        #[arg(long)] all_versions: bool,
        #[arg(long)] all: bool,
    },
}
```

Handler `cmd_path` (async, `#[tokio::main]` or `pollster::block_on` to avoid adding tokio to the whole binary — prefer `tokio` with `rt-multi-thread` feature gated to this command, following `crates/git`'s blocking style as alternative: use `reqwest::blocking` to stay sync):
- Parse spec, call `package::resolve_and_materialize` (which checks cache, else fetch+extract).
- On success: if `--json` print JSON object (single line), else print `path.display()` to stdout. Stderr diagnostics via `eprintln!` or `tracing` when `--verbose`.
- On `--evict` short-circuit to `package::evict` and exit 0.
- Map `PackageError` to `std::process::exit(code)` per PRODUCT.md; do not use `anyhow::bail!` which would exit 1 and lose code.

Add `crates/cli/Cargo.toml` deps: `crates/package = { path = "../package" }`, `tokio` (optional), `reqwest`, `dirs`, `fs2`, `sha2`, `tar`, `flate2`, `zip`.

### 3. Follow existing patterns

- Error handling mirrors [`crates/config/src/error.rs:1 @ 4f91405`](https://github.com/nrelab/RepoSync/blob/4f9140588d9bc7cb3b07e3bafa82214fa4e65bd1/crates/config/src/error.rs#L1) (`thiserror`) and CLI's `anyhow::Context` at [`crates/cli/src/main.rs:4 @ 4f91405`](https://github.com/nrelab/RepoSync/blob/4f9140588d9bc7cb3b07e3bafa82214fa4e65bd1/crates/cli/src/main.rs#L4).
- No `unsafe`; respect `unsafe_code = forbid`.

### 4. Alternatives considered

- Reuse `npm`/`pip` CLI instead of direct registry HTTP: rejected — requires toolchains on host, non-hermetic, version resolution differs.
- Store packages as Git repos (like `crates/git`): rejected — registries are tarball-based; Git would add overhead.
- Sync `reqwest` vs `ureq`: `reqwest` chosen for `rustls` consistency and JSON; `reqwest::blocking` avoids async runtime if desired. Document trade-off in code.

## End-to-end flow

```
user: reposync path zod@3.22.4
  -> Cli::Path { spec="zod@3.22.4" } @ crates/cli/src/main.rs:31
  -> PackageSpec::parse -> Registry::Npm, name="zod", version="3.22.4"
  -> cache_root()/packages/npm/zod/3.22.4/package exists? -> yes: print path
  -> no: NpmClient::resolve (GET https://registry.npmjs.org/zod) -> ResolvedPackage
      -> NpmClient::fetch (GET tarball, verify sha) -> cache::materialize (tmp -> rename, .complete)
      -> print path
shell: rg "parse" $(reposync path zod) -> rg runs on printed dir
```

## Testing and validation

Maps to `PRODUCT.md` Behavior invariants; validation lives here per spec (PRODUCT.md has no Validation section).

- **Unit** (`crates/package/src/spec.rs`): `PRODUCT.md#1-2` — bare `zod` → Npm, `pypi:requests` → Pypi, `zod@1.2.3` vs `--version` mismatch → error, unsupported `crates:serde` → error.
- **Unit** (`cache::cache_root`): `PRODUCT.md#4` — respects `$REPOSYNC_CACHE`, `$XDG_CACHE_HOME`, fallback; layout `packages/<registry>/<name>/<version>/package`.
- **Integration** (wiremock/httpmock): `PRODUCT.md#3,5,10` — npm resolve latest, PyPI resolve, network failure on miss → exit 4, cached offline → success with stderr note, integrity mismatch → exit 5 and removal, concurrent `resolve_and_materialize` on same spec via `#[tokio::test]` with barrier → no corruption.
- **Extraction** (`src/extract.rs`): `PRODUCT.md#13` — tar with `../../etc/passwd` and symlink escape are rejected; permissions capped.
- **CLI e2e** (`crates/cli/tests/path.rs`): `PRODUCT.md#6-9` — stdout is exactly one absolute path (no extra lines), `--json` yields one JSON object with `path/registry/version/cached`, `rg`/`cat`/`find` examples from PRODUCT.md work against the printed path (create temp fixture via mock registry, run `rg` as subprocess). Each error case asserts exit code 2-6 and stderr substring per `PRODUCT.md#10`.
- **Cache semantics** (`PRODUCT.md#11-12`): second call returns identical path, `--evict` removes entry and forces re-fetch (mock server hit count +1).
- **Manual**: `cargo run -p reposync-cli -- path zod --verbose` (online), then offline with `unshare -n` or `REPOSYNC_CACHE` pointing to populated dir, and `find $(cargo run -- path pypi:requests) -name "*.py" | head`.

## Parallelization

Sub-agents would not meaningfully reduce wall-clock time. The work is tightly coupled around a single new crate and one CLI command; splitting registry clients (npm vs PyPI) across agents would duplicate cache/extraction design and require a merge point on `PackageSpec`/`cache` ownership.

**Proposed: single local agent** on `main` (worktree: `/Users/khulnasoft/axilo`), branch `feat/reposync-path`, single PR containing `PRODUCT.md`/`TECH.md` plus `crates/package` and `crates/cli` changes. If the implementer later decides to parallelize, split only after `PackageSpec`+`cache` are landed: agent A (local, `crates/package` npm client), agent B (local, PyPI client), coordinated via `crates/package/src/registry.rs` trait.

## Risks and mitigations

- **Cache poisoning / partial writes:** Mitigated by temp-dir + atomic rename and `.complete` sentinel; `Integrity` error removes entry.
- **Registry schema drift:** Npm/PyPI JSON fields may change; pin to documented fields (`dist.tarball`, `info.version`, `urls[].digests`) and add wiremock regression tests that snapshot real responses.
- **Large tarballs / disk pressure:** No TTL in v1 (`PRODUCT.md#12`); add `evict --all` and document `REPOSYNC_CACHE` on tmpfs warning. Future `--max-cache-size` deferred.
- **Tar-slip:** Covered by extraction sandbox tests; fuzz with `cargo fuzz` on `extract` using crafted archives.

## Follow-ups

- `go:`/`crates:` registries (PRODUCT.md#2 open).
- `packages:` stanza in `reposync.yaml` for declarative prefetch.
- `--refresh` / `--max-cache-size` and `sdist` vs `wheel` preference flag for PyPI (open question).
