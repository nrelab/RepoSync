---
name: package-source
description: Resolves any registry package to its unpacked source directory via `reposync path`. Use when an agent needs to search, read, or enumerate package sources with `rg`, `cat`, or `find` (e.g. `rg "parse" $(reposync path zod)`, `cat $(reposync path zod)/src/types.ts`, `find $(reposync path pypi:requests) -name "*.py"`). Supports npm and pypi with shell-composable paths.
---

# Package Source

Give coding agents uniform, offline-friendly access to any package's source via `reposync path`.

## Instructions

### Step 1: Resolve the package to a path

Use `reposync path` to materialize the package. Do not guess install locations or use `node_modules`/`site-packages` directly.

```bash
# npm (default)
reposync path zod
reposync path zod@3.22.4
reposync path @scope/pkg@1.0.0
reposync path npm:express@4.18.2

# PyPI
reposync path pypi:requests
reposync path pypi:requests==2.31.0
reposync path pypi:requests@2.31.0

# Flags
reposync path zod --json        # {"registry":"npm","name":"zod","version":"3.22.4","path":"...","cached":true}
reposync path zod --verbose     # progress to stderr, path still to stdout
reposync path zod --evict       # force re-fetch next time
reposync path zod --evict --all-versions
reposync path --evict --all     # clear all cached packages
```

Precedence: inline version (`zod@1.0.0` / `pypi:requests==2.31.0`) vs `--version` must agree or the command exits `2`. Bare names default to `npm`; `pypi:` forces PyPI. Other registries (`crates:`, `go:`) error with `unsupported registry`.

### Step 2: Use standard Unix tools on the path

`reposync path` prints exactly one absolute path (newline-terminated) to stdout; all diagnostics go to stderr. This makes it composable:

```bash
# Search
rg "parse" $(reposync path zod)
rg --type ts "interface" $(reposync path zod)

# Read a specific file
cat $(reposync path zod)/src/types.ts
bat $(reposync path pypi:requests)/requests/api.py

# Enumerate
find $(reposync path pypi:requests) -name "*.py" | head
ls $(reposync path zod)/src

# JSON mode for programmatic use
path=$(reposync path zod --json | jq -r .path)
```

### Step 3: Handle cache and offline

- Cache root: `$REPOSYNC_CACHE` → `$XDG_CACHE_HOME/reposync` → `~/.cache/reposync`, layout `packages/<registry>/<normalized-name>/<version>/package`.
- Second call for the same `<registry>/<name>/<version>` is instant (no network). Concurrent callers are serialized via file lock.
- If the version is already cached, the command works fully offline (`using cached ... (offline)` to stderr with `--verbose`).
- To force freshness: `reposync path <spec> --evict` or `REPOSYNC_CACHE=/tmp/cache reposync path ...` for CI isolation.

### Step 4: Handle errors by exit code

- `2` invalid spec / unsupported registry (`supported registries: npm, pypi`)
- `3` not found (`package 'x' not found`, `version 'y' not found` — lists 5 recent versions when available)
- `4` network failure on cache miss (`failed to fetch ...`)
- `5` integrity mismatch (`integrity check failed ...`, cache entry removed)
- `6` cache I/O / permission (`hint: set $REPOSYNC_CACHE`)

On error, do not treat empty stdout as a valid path. Check `exit code != 0` and surface `stderr`.

### References

- Spec: `specs/reposync-path/PRODUCT.md` (behavior 1–15) and `specs/reposync-path/TECH.md` (cache/registry/extraction)
- Implementation: `crates/package/src/spec.rs:30`, `crates/package/src/cache.rs:8`, `crates/package/src/registry.rs:30`, `crates/cli/src/main.rs:672`
