# Package Source Access

`reposync path` resolves npm and pypi package specs to local cached directories, providing offline-friendly access to package source code for tools like `rg`, `cat`, and editors.

## Overview

Package managers install to project-local, registry-specific, and version-scattered locations. `reposync path` gives you a single, predictable primitive to access any package's unpacked source:

```bash
rg "parse" $(reposync path zod)
cat $(reposync path zod)/src/types.ts
find $(reposync path pypi:requests) -name "*.py"
```

## Usage

### Basic Resolution

Resolve the latest version of a package:

```bash
reposync path zod
# Output: /Users/user/.cache/reposync/packages/npm/zod/4.6.5/package
```

### Specific Version

Resolve a specific version:

```bash
reposync path zod@3.22.4
reposync path pypi:requests==2.31.0
```

### Registry Prefix

Explicitly specify the registry:

```bash
reposync path npm:zod
reposync path pypi:requests
```

### JSON Output

Get structured metadata with the path:

```bash
reposync path --json zod
# Output: {"cached":true,"name":"zod","path":"/Users/user/.cache/reposync/packages/npm/zod/4.6.5/package","registry":"npm","version":"4.6.5"}
```

### Verbose Mode

See cache vs fetch behavior:

```bash
reposync path --verbose zod
# Output: reposync: fetching zod@4.6.5 from https://registry.npmjs.org/zod/-/zod-4.6.5.tgz
#         reposync: resolved zod -> 4.6.5
#         /Users/user/.cache/reposync/packages/npm/zod/4.6.5/package
```

### Cache Management

Evict a specific version:

```bash
reposync path --evict zod@3.22.4
```

Evict all versions of a package:

```bash
reposync path --evict --all-versions zod
```

Evict all cached packages:

```bash
reposync path --evict --all
```

## Registry Support

### npm

Default registry for bare names. Supports scoped packages:

```bash
reposync path zod
reposync path @scope/package
reposync path @scope/package@1.0.0
```

### PyPI

Use the `pypi:` prefix. Supports both `==` and `@` for version specification:

```bash
reposync path pypi:requests
reposync path pypi:requests==2.31.0
reposync path pypi:requests@2.31.0
```

### Future Registries

Other registries (`crates:`, `go:`, `maven:`) are not yet supported and will return an error listing supported registries.

## Cache Layout

Packages are cached under the reposync cache root:

```
$REPOSYNC_CACHE (or $XDG_CACHE_HOME/reposync or ~/.cache/reposync)
└── packages/
    ├── npm/
    │   └── zod/
    │       ├── 3.22.4/
    │       │   ├── package/
    │       │   │   ├── package.json
    │       │   │   ├── src/
    │       │   │   └── ...
    │       │   └── .complete
    │       └── 4.6.5/
    │           └── ...
    └── pypi/
        └── requests/
            └── 2.34.2/
                └── ...
```

The `.complete` sentinel file marks a fully downloaded and extracted entry.

## Cache Environment Variables

Set the cache location:

```bash
export REPOSYNC_CACHE=/path/to/custom/cache
reposync path zod
```

Or use `$XDG_CACHE_HOME/reposync` (default: `~/.cache/reposync`).

## Error Handling

### Exit Codes

| Code | Condition |
|------|-----------|
| 0 | Success |
| 2 | Invalid spec, unsupported registry, or configuration error |
| 3 | Package or version not found |
| 4 | Network failure (on cache miss) |
| 5 | Integrity check failed |
| 6 | Permission denied writing cache |

### Error Messages

```bash
# Unknown package
reposync path nonexistentpackage12345
# Output: error: package 'nonexistentpackage12345' not found in registry 'npm'

# Unknown version
reposync path zod@999.999.999
# Output: error: version '999.999.999' not found for 'zod'

# Unsupported registry
reposync path crates:serde
# Output: error: unsupported registry 'crates'
#         supported registries: npm, pypi

# Empty spec
reposync path ""
# Output: error: package spec must not be empty
```

## Security

- **HTTPS only:** Only HTTPS URLs are accepted; HTTP is rejected
- **Sandboxed extraction:** Tarball extraction rejects absolute paths, `..` traversal, and symlinks escaping the package root
- **Integrity verification:** npm packages are verified via `integrity`/`shasum`; PyPI packages via `sha256` digests
- **No execution:** The command does not run build scripts or lifecycle hooks

## Integration with Tools

### ripgrep

Search package source:

```bash
rg "parse" $(reposync path zod)
```

### cat

Read specific files:

```bash
cat $(reposync path zod)/src/types.ts
```

### find

Enumerate files:

```bash
find $(reposync path pypi:requests) -name "*.py"
```

### Editors

Open in your editor:

```bash
vim $(reposync path zod)/src/lib.rs
code $(reposync path zod)
```

## Shell Composability

The command is designed for shell substitution:

```bash
# Build from package source
cd $(reposync path zod) && npm run build

# Run tests
cd $(reposync path pypi:requests) && python -m pytest

# Archive source
tar -czf zod-source.tar.gz -C $(reposync path zod) .
```

## Offline Usage

Once a package is cached, `reposync path` works fully offline:

```bash
# First run (online)
reposync path zod

# Later runs (offline)
offline-reposync path zod  # Still works, uses cache
```

## Version Resolution

When no version is specified, the command resolves to the registry's default version:

- **npm:** `dist-tag latest`
- **PyPI:** `info.version` (the latest stable version)

The resolved version is part of the cache key, so you can pin versions explicitly for reproducibility:

```bash
# Always get 3.22.4
reposync path zod@3.22.4
```

## Performance

- **Cache hit:** Instant (no network I/O)
- **Cache miss:** Network-bound (fetch tarball, verify integrity, extract)
- **Concurrent access:** Multiple callers racing on the same spec are serialized; one wins, others reuse the result

## Best Practices

1. **Pin versions in scripts:** Use explicit versions for reproducibility
2. **Use JSON in scripts:** `--json` provides structured output for parsing
3. **Set REPOSYNC_CACHE in CI:** Share cache across CI steps
4. **Evict before debugging:** Use `--evict` to force re-fetch when investigating issues
5. **Prefer npm over PyPI for TypeScript:** npm packages have better source code structure for searching

## Examples

### CI/CD Pipeline

```yaml
steps:
  - name: Cache package sources
    run: |
      reposync path zod@3.22.4
      reposync path typescript@5.3.3
  
  - name: Build from cached sources
    run: |
      cd $(reposync path zod@3.22.4)
      npm run build
```

### Local Development

```bash
# Explore a package's source
cd $(reposync path zod)
code .

# Compare versions
diff -u $(reposync path zod@3.22.4)/src $(reposync path zod@4.6.5)/src
```

### Automation

```bash
# Download all dependencies of a project
for dep in $(cat package.json | jq -r '.dependencies | keys[]'); do
  reposync path "$dep"
done
```

## Troubleshooting

### Cache Corruption

If you suspect cache corruption, evict and re-fetch:

```bash
reposync path --evict zod
reposync path zod
```

### Permission Errors

If you see permission denied errors:

```bash
# Check cache directory permissions
ls -la ~/.cache/reposync

# Set custom cache location with write access
export REPOSYNC_CACHE=/tmp/reposync-cache
```

### Network Issues

If fetching fails (exit code 4), check:
- Internet connectivity
- Registry URL is accessible
- No proxy or firewall blocking the connection

Cache entries that fail integrity check are automatically removed.
