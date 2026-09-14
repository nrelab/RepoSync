# PRODUCT.md — `reposync path` — Package Source Access for Coding Agents

## Summary

Coding agents need to inspect the source of any third-party package to answer questions about types, APIs, and implementation. `reposync path <spec>` gives them a local filesystem path to a package's unpacked source so standard Unix tools (`rg`, `cat`, `find`, editors) work without network calls or custom APIs. The command is shell-composable: `rg "parse" $(reposync path zod)`.

## Problem

Agents frequently need to read package internals (`zod` types, `requests` implementation) but have no uniform, offline-friendly way to locate source. Package managers install to project-local, registry-specific, and version-scattered locations; fetching ad-hoc is slow and non-cacheable. Existing `reposync` already clones Git repositories to deterministic local paths — extending the same model to registry packages gives agents a single, predictable primitive.

## Goals / Non-goals

**Goals:**
- One command that resolves any registry package to an absolute, already-unpacked directory path.
- Shell-composable stdout (only the path, newline-terminated) for `$(...)` substitution.
- Deterministic, content-addressable cache; repeated calls are instant when cached, network-bound only on miss.
- Explicit errors (not silent empty output) for unknown packages/versions.

**Non-goals:**
- Not a package manager: does not install dependencies, run build/lifecycle scripts, or modify the caller's project (`package.json`, `requirements.txt`).
- Not a search/discovery service: does not rank or recommend packages; caller provides the exact spec.
- Not a code-execution sandbox.

## Behavior

1. **Command shape.** `reposync path <spec> [--version <ver>] [--registry <name>]` (and short `-r`/`-v`) resolves a package spec to a local directory and prints its absolute path to stdout followed by `\n` and exits `0`. `<spec>` is `[@scope/]name` (npm default) or `registry:name` (e.g. `pypi:requests`, `npm:zod`). Version may be inline (`zod@3.22.4`, `pypi:requests==2.31.0`, `pypi:requests@2.31.0`) or via `--version`. If both are given they must agree or the command fails.

2. **Registry routing.** Bare names default to `npm`. Prefix `npm:` forces npm, `pypi:` forces PyPI. Other registries (`crates:`, `go:`) return a user-facing error `unsupported registry '<name>'` with exit `2` until implemented; the error lists supported registries.

3. **Default version.** When no version is supplied, the command resolves to the registry's `latest`/`stable` (npm `dist-tag latest`, PyPI `info.version`). The resolved version is part of the cache key and is printed to stderr as `reposync: resolved <spec> -> <version>` when `--verbose` is set; without `--verbose`, stdout remains path-only.

4. **Cache location and layout.** Each resolved package is unpacked under the reposync cache root (`$REPOSYNC_CACHE` if set, else `$XDG_CACHE_HOME/reposync` else `~/.cache/reposync`) at `<root>/packages/<registry>/<normalized-name>/<version>/package/`. Example: `$(reposync path zod)` → `~/.cache/reposync/packages/npm/zod/3.22.4/package`. Re-invoking the same spec reuses the existing directory without network I/O.

5. **Fetch-on-miss.** On cache miss the command fetches the package tarball/sdist from the registry, verifies its integrity (npm `integrity`/`shasum`, PyPI `sha256`/`digests`), unpacks to a temporary directory, then atomically moves into place. Concurrent callers racing on the same `<registry>/<name>/<version>` do not corrupt the cache; one wins, others reuse the winner or retry once on transient move failure.

6. **Stdout contract.** Stdout contains exactly one line: the absolute, normalized path (no trailing slash, no `.`/`..` segments). No other text goes to stdout. Stderr carries all diagnostics (`fetching …`, `using cache …`, warnings). This guarantees `rg "parse" $(reposync path zod)` works with word-splitting-safe paths and `cat $(reposync path zod)/src/types.ts` works via simple concatenation.

7. **Path validity.** The printed path always exists, is a directory, is readable, and contains the package's top-level files as published (e.g. `package.json` for npm, `setup.py`/`pyproject.toml` or top-level `requests/` for PyPI sdist). If the package would be empty, the command still prints a valid directory but also emits a warning to stderr.

8. **Listing and inspection.** Flags:
   - `--json` prints `{"registry":"npm","name":"zod","version":"3.22.4","path":"…","cached":true}` to stdout instead of the bare path (still exactly one JSON object, newline-terminated).
   - `--verbose`/`-v` emits structured progress to stderr.
   - `--help` shows usage with registry examples; `--version` (global) still prints reposync version and is not confused with package version — package version must be passed as `--version` *after* `path` or inline with `@`/`==`.

9. **Search/read ergonomics.** After resolution, standard tools operate on the path without reposync involvement:
   - `rg "parse" $(reposync path zod)` searches the package source.
   - `cat $(reposync path zod)/src/types.ts` reads a specific file.
   - `find $(reposync path pypi:requests) -name "*.py"` enumerates files.
   The command does not wrap or proxy these tools; it only supplies the path.

10. **Error handling.**
    - Unknown package name → stderr `package '<name>' not found in registry '<registry>'`, exit `3`.
    - Unknown version → stderr `version '<ver>' not found for '<name>'`, exit `3`, and lists the 5 most recent available versions when the registry provides them.
    - Network failure / offline on cache miss → stderr `failed to fetch …: <cause>`, exit `4`; if the same spec is already cached, offline succeeds from cache with a stderr note `using cached <version> (offline)`.
    - Integrity mismatch → stderr `integrity check failed for <name>@<ver>`, cache entry is removed, exit `5`.
    - Permission denied writing cache → stderr with the path and OS error, exit `6` with a hint to set `$REPOSYNC_CACHE`.
    - Multiple specs (`reposync path zod pypi:requests`) is not supported in v1; the command errors `path takes exactly one package spec` with exit `2` and suggests invoking once per package.

11. **Idempotence and caching.** Repeated `reposync path <same-spec>` with no intervening `reposync path --evict <spec>` returns the identical absolute path bit-for-bit. Cache entries are immutable once written; the command never mutates an existing `package/` directory. `--evict <spec>` (or `--evict --all`) removes cached entries and is the only way to force a re-fetch.

12. **Eviction and freshness.** `--evict` removes the specific `<registry>/<name>/<version>` directory (or all versions of a name with `pypi:requests --all-versions`). After eviction, the next `reposync path` re-fetches. There is no implicit TTL; freshness is explicit via eviction or an explicit version bump. A future `--refresh` may be added but is out of scope for v1.

13. **Security.** Tarball extraction is sandboxed: no absolute paths, no `..` traversal, no symlinks escaping the `package/` root, no executable permission escalation beyond `0o644`/`0o755`. Registry URLs are HTTPS only; HTTP is rejected with exit `2`.

14. **Offline and CI.** When the cache already contains the requested version, the command works fully offline (no DNS/HTTP). In CI, setting `REPOSYNC_CACHE` to a workspace-persistent directory makes the cache shareable across steps.

15. **Composition with existing reposync.** `reposync path` is independent of `reposync validate/plan/migrate/diff/sync` and does not require a `reposync.yaml`. It shares only the cache root and the `--verbose`/`--json` flag conventions. Future `reposync.yaml` may declare `packages:` for prefetch, but v1 `path` is imperative/CLI-only.

**Open question:** Should `pypi:` sdist vs wheel be preferred when both exist, and should `reposync path pypi:requests --prefer wheel` be supported for agents that need built artifacts? For v1, prefer sdist (source) for `find`/`rg` over Python files; document that wheels may contain no `.py` sources.
