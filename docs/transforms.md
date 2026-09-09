# Transform reference

Transforms execute in YAML order against an in-memory snapshot. A failed step
stops the pipeline and leaves the destination untouched.

## Path transforms

| Transform | Fields | Behavior |
| --- | --- | --- |
| `filter` | `paths` | Keeps only matching paths. |
| `delete` | `paths` | Removes matching paths. |
| `copy` | `from`, `to` | Copies a path or subtree and keeps the original. |
| `move` | `from`, `to` | Relocates a path or subtree; collisions fail. |
| `rename` | `from`, `to` | Renames a path prefix; collisions fail. |
| `strip_prefix` | `path` | Removes a leading path prefix. |

Path patterns use `*` for characters within one path segment, `?` for one
character, and `**` across path segments.

## Content transforms

| Transform | Fields | Behavior |
| --- | --- | --- |
| `replace` | `file`, `replacements` | Applies literal substitutions in one file. |
| `regex_replace` | `files`, `pattern`, `replacement` | Replaces regex matches across selected files. |
| `prepend` | `files`, `content` | Adds content at the beginning of selected files. |
| `append` | `files`, `content` | Adds content at the end of selected files. |
| `patch` | `file`, `patch` | Applies a unified-style patch to one file. |

Binary or missing files are rejected when a content transform cannot safely
decode or locate its input.

## Structured transforms

`dependency_rewrite` rewrites package names and can replace `workspace:*`
versions. `import_rewrite` rewrites module specifiers in import and require
forms. `metadata` adds custom metadata to the output snapshot.

Example:

```yaml
- dependency_rewrite:
    package_rename:
      "@internal/sdk": "@public/sdk"
    workspace_version: "1.2.3"
- import_rewrite:
    renames:
      "@internal/sdk": "@public/sdk"
```

## History-only transforms

`author_mapping` and `commit_message` apply during `migrate --history` and do
not alter snapshot-mode file output.

```yaml
- author_mapping:
    mapping:
      old@example.com: new@example.com
- commit_message:
    message: "[public] "
```

## Custom WASM transforms

Use `custom` to load a `.wasm` module:

```yaml
- custom:
    name: rename-prefix
    path: transformations/examples/rename-prefix.wasm
    args:
      prefix: public-
```

The module must export `memory`, `alloc`, and `transform`. Input and output are
JSON envelopes containing a serialized repository snapshot and configured
arguments. See `crates/transform/src/custom.rs` and the plugin integration test
for the ABI details.
