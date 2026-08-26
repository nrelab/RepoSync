use std::path::PathBuf;

use reposync_config::{ConfigFile, TransformKind, ValidationError};

fn examples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

fn load_example(name: &str) -> ConfigFile {
    ConfigFile::load(&examples_dir().join(name)).unwrap_or_else(|error| {
        panic!("failed to load {name}: {error}");
    })
}

fn messages(errors: &[ValidationError]) -> Vec<&str> {
    errors.iter().map(ValidationError::message).collect()
}

#[test]
fn ok_example_passes_validation() {
    let config = load_example("ok.yaml");
    let errors = config.validate();
    assert!(errors.is_empty(), "{errors:#?}");
}

#[test]
fn ok_example_uses_tier_one_transforms() {
    let config = load_example("ok.yaml");
    let kinds: Vec<TransformKind> = config
        .transform
        .iter()
        .map(|node| node.kind().unwrap())
        .collect();
    assert_eq!(
        kinds,
        vec![
            TransformKind::Filter,
            TransformKind::Move,
            TransformKind::Delete
        ]
    );
}

#[test]
fn bad_example_reports_actionable_errors() {
    let config = load_example("bad.yaml");
    let errors = config.validate();
    let messages = messages(&errors);

    assert!(
        messages.contains(&"pipeline.name must not be empty"),
        "missing name error: {messages:#?}"
    );
    assert!(
        messages.contains(&"source.ref must not be empty"),
        "missing ref error: {messages:#?}"
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("filter.paths must not be empty")),
        "missing filter error: {messages:#?}"
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("move.from must not equal")),
        "missing move error: {messages:#?}"
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("copy.from must not be empty")),
        "missing copy error: {messages:#?}"
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("same repository as source")),
        "missing same-repo error: {messages:#?}"
    );
}

#[test]
fn monorepo_example_no_longer_gates_implemented_kinds() {
    let config = load_example("monorepo-sdk.yaml");
    let errors = config.validate();
    assert!(
        errors
            .iter()
            .all(|e| !e.message().contains("not yet implemented")),
        "implemented transforms should not be gated: {errors:#?}"
    );
    assert!(errors.is_empty(), "{errors:#?}");
}

#[test]
fn unknown_transform_is_a_parse_error() {
    let result = ConfigFile::load(&examples_dir().join("unknown-transform.yaml"));
    assert!(result.is_err());
}

#[test]
fn same_repo_detected_across_normalization() {
    let config = ConfigFile::from_yaml(
        r"
pipeline:
  name: x
source:
  type: git
  url: git@github.com:acme/repo.git
  ref: main
transform: []
destination:
  type: git
  url: git@github.com:acme/repo.git/
  branch: main
",
    )
    .unwrap();
    let errors = config.validate();
    assert!(errors
        .iter()
        .any(|e| e.message().contains("same repository")));
}

#[test]
fn move_to_root_is_valid() {
    let config = ConfigFile::from_yaml(
        r"
pipeline:
  name: x
source:
  type: git
  url: git@github.com:acme/monorepo.git
  ref: main
transform:
  - move:
      from: packages/sdk
      to: .
destination:
  type: git
  url: git@github.com:acme/public-sdk.git
  branch: main
",
    )
    .unwrap();
    let errors = config.validate();
    assert!(errors.is_empty(), "{errors:#?}");
}

#[test]
fn path_prefix_rejects_absolute_and_dotdot() {
    let config = ConfigFile::from_yaml(
        r"
pipeline:
  name: x
source:
  type: git
  url: git@github.com:acme/monorepo.git
  ref: main
transform:
  - move:
      from: /abs
      to: .
  - rename:
      from: a
      to: ../escape
destination:
  type: git
  url: git@github.com:acme/public-sdk.git
  branch: main
",
    )
    .unwrap();
    let errors = config.validate();
    assert!(
        errors
            .iter()
            .any(|e| e.message().contains("must be a relative path")),
        "{errors:#?}"
    );
    assert!(
        errors
            .iter()
            .any(|e| e.message().contains("must not contain '..'")),
        "{errors:#?}"
    );
}

#[test]
fn node_with_two_transforms_is_rejected() {
    let config = ConfigFile::from_yaml(
        r"
pipeline:
  name: x
source:
  type: git
  url: git@github.com:acme/monorepo.git
  ref: main
transform:
  - filter:
      paths: [a]
    move:
      from: a
      to: b
destination:
  type: git
  url: git@github.com:acme/public-sdk.git
  branch: main
",
    )
    .unwrap();
    let errors = config.validate();
    assert!(
        errors
            .iter()
            .any(|e| e.message().contains("exactly one transformation")),
        "{errors:#?}"
    );
}
