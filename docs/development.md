# Development guide

## Local checks

Run the complete test suite from the repository root:

```bash
cargo test --workspace
```

Before committing, run formatting and lint checks:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

The test suite includes unit tests for the core model and transforms, Git tree
round trips, configuration validation, policy checks, migration history,
bidirectional sync, plugin execution, and end-to-end CLI scenarios.

## Adding a transform

1. Add the argument type and YAML field in `crates/config/src/transform.rs`.
2. Add semantic validation in `crates/config/src/validate.rs`.
3. Implement `Transformation` in `crates/transform/src/`.
4. Export the implementation from `crates/transform/src/lib.rs`.
5. Construct it in `crates/cli/src/main.rs`.
6. Add unit, integration, and configuration example coverage.

Transforms should be deterministic, operate only on `TransformContext`, and
return structured change events. Avoid direct filesystem or Git writes inside a
transform.

## Adding Git behavior

Keep Git operations in `crates/git`. Read and write snapshots through the
`GitRepo` abstraction rather than exposing libgit2 details to transformation
code. Add a focused integration test under `crates/git/tests/` for every new
ref, tree, commit, or push behavior.

## Plugin development

Plugins communicate through JSON serialized in WASM linear memory. The host
requires the exports `memory`, `alloc`, and `transform`. A plugin receives a
snapshot plus its configured arguments and returns a transformed snapshot.

Keep plugins deterministic and avoid relying on host filesystem, network, or
process state. The plugin tests in `crates/transform/tests/plugin.rs` provide a
minimal ABI fixture.

## Suggested workflow

```bash
git status
cargo test --workspace
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
git diff --check
```

Update `PLAN.md` when a milestone or definition of done changes.
