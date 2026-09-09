# Configuration reference

RepoSync configuration is YAML with four required sections:

```yaml
pipeline:
  name: public-sdk
source:
  type: git
  url: /path/to/source
  ref: main
transform: []
destination:
  type: git
  url: /path/to/destination
  branch: main
```

Unknown keys are rejected. Every transform entry must contain exactly one
transform kind, and semantic validation runs before repositories are opened.

## Source and destination

Both providers currently use `type: git`. Locations may be local paths, SSH,
HTTPS, `git://`, or `file://` URLs.

- `source.ref` selects a branch, tag, or commit to read.
- `destination.branch` selects the branch to update. An empty destination is
  initialized on this branch when the first commit is written.

Source and destination must not resolve to the same normalized repository URL.

## Policies

The optional `policy` section gates writes:

```yaml
policy:
  deny:
    - "*.pem"
    - .env
    - "**/credentials/**"
  require_review:
    - package.json
  max_deleted_files: 100
```

- `deny` rejects output paths matching any listed glob.
- `require_review` reports paths that need review and blocks the run until the
  policy is changed or an approval flow is added.
- `max_deleted_files` limits removed destination files in one run.

## State

Snapshot and history migrations use SQLite state at `.sync/state.db` by
default. Override it with `--state path/to/state.db`. The state database maps
source commit IDs to generated destination commit IDs and makes repeated runs
incremental.

## Validation

Use validation before planning or migration:

```bash
reposync validate pipeline.yaml
```

The command reports all schema and semantic errors it finds, including invalid
Git locations, empty refs, invalid paths, transform collisions, and unknown
transform keys.
