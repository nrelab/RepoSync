use std::fmt;

use crate::config::ConfigFile;
use crate::policy::Policy;
use crate::source::{is_valid_git_url, normalize_url, Destination, Source};
use crate::transform::{
    AppendArgs, AuthorMappingArgs, CommitMessageArgs, CustomArgs, DeleteArgs, DependencyRewriteArgs,
    EngineSupport, FilterArgs, ImportRewriteArgs, MetadataArgs, PatchArgs, PrependArgs,
    RegexReplaceArgs, ReplaceArgs, StripPrefixArgs, TransformKind, TransformNode,
};

/// A single problem found during semantic validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    message: String,
}

impl ValidationError {
    /// Creates a validation error with a human-readable message.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// Returns the error message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ValidationError {}

/// Runs all semantic checks over a parsed configuration.
///
/// Returns every problem found; an empty vector means the configuration is
/// valid. Parsing-level problems (unknown keys, malformed YAML) are reported
/// by [`ConfigFile::load`](crate::ConfigFile::load) instead.
#[must_use]
pub fn validate(config: &ConfigFile) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    validate_pipeline(config, &mut errors);
    validate_source(&config.source, &mut errors);
    validate_destination(&config.destination, &mut errors);
    validate_distinct_repos(config, &mut errors);

    for (index, node) in config.transform.iter().enumerate() {
        validate_transform(index, node, &mut errors);
    }

    if let Some(policy) = &config.policy {
        validate_policy(policy, &mut errors);
    }

    errors
}

fn validate_pipeline(config: &ConfigFile, errors: &mut Vec<ValidationError>) {
    if let Some(name) = &config.pipeline.name {
        if name.trim().is_empty() {
            errors.push(ValidationError::new("pipeline.name must not be empty"));
        }
    }
}

fn validate_source(source: &Source, errors: &mut Vec<ValidationError>) {
    let Source::Git { url, ref_name } = source;
    if url.trim().is_empty() {
        errors.push(ValidationError::new("source.url must not be empty"));
    } else if !is_valid_git_url(url) {
        errors.push(ValidationError::new(format!(
            "source.url `{url}` is not a recognized git URL (local path, SSH, or HTTPS)"
        )));
    }
    if ref_name.trim().is_empty() {
        errors.push(ValidationError::new("source.ref must not be empty"));
    }
}

fn validate_destination(destination: &Destination, errors: &mut Vec<ValidationError>) {
    let Destination::Git { url, branch } = destination;
    if url.trim().is_empty() {
        errors.push(ValidationError::new("destination.url must not be empty"));
    } else if !is_valid_git_url(url) {
        errors.push(ValidationError::new(format!(
            "destination.url `{url}` is not a recognized git URL (local path, SSH, or HTTPS)"
        )));
    }
    if branch.trim().is_empty() {
        errors.push(ValidationError::new("destination.branch must not be empty"));
    }
}

fn validate_distinct_repos(config: &ConfigFile, errors: &mut Vec<ValidationError>) {
    if normalize_url(config.source.url()) == normalize_url(config.destination.url()) {
        errors.push(ValidationError::new(format!(
            "destination must not point at the same repository as source (`{}`)",
            config.source.url()
        )));
    }
}

#[allow(clippy::too_many_lines)]
fn validate_transform(index: usize, node: &TransformNode, errors: &mut Vec<ValidationError>) {
    let count = node.kind_count();
    let kind = match node.kind() {
        Some(kind) if count == 1 => kind,
        Some(_) => {
            errors.push(ValidationError::new(format!(
                "transform[{index}]: must specify exactly one transformation, found {count}"
            )));
            return;
        }
        None => {
            errors.push(ValidationError::new(format!(
                "transform[{index}]: must specify exactly one transformation (e.g. `filter`)"
            )));
            return;
        }
    };

    if let EngineSupport::NotYetImplemented(reason) = kind.engine_support() {
        errors.push(ValidationError::new(format!(
            "transform[{index}]: `{kind}` is not yet implemented ({reason})"
        )));
    }

    match kind {
        TransformKind::Filter => {
            validate_filter(
                index,
                node.filter.as_ref().expect("kind filter implies args"),
                errors,
            );
        }
        TransformKind::Delete => {
            validate_delete(
                index,
                node.delete.as_ref().expect("kind delete implies args"),
                errors,
            );
        }
        TransformKind::Copy => {
            let args = node.copy.as_ref().expect("kind copy implies args");
            validate_from_to(index, "copy", &args.from, &args.to, false, errors);
        }
        TransformKind::Move => {
            let args = node.r#move.as_ref().expect("kind move implies args");
            validate_from_to(index, "move", &args.from, &args.to, true, errors);
        }
        TransformKind::Rename => {
            let args = node.rename.as_ref().expect("kind rename implies args");
            validate_from_to(index, "rename", &args.from, &args.to, false, errors);
        }
        TransformKind::Replace => {
            validate_replace(
                index,
                node.replace.as_ref().expect("kind replace implies args"),
                errors,
            );
        }
        TransformKind::RegexReplace => {
            validate_regex_replace(
                index,
                node.regex_replace
                    .as_ref()
                    .expect("kind regex_replace implies args"),
                errors,
            );
        }
        TransformKind::StripPrefix => {
            validate_strip_prefix(
                index,
                node.strip_prefix
                    .as_ref()
                    .expect("kind strip_prefix implies args"),
                errors,
            );
        }
        TransformKind::Prepend => {
            validate_prepend(
                index,
                node.prepend.as_ref().expect("kind prepend implies args"),
                errors,
            );
        }
        TransformKind::Append => {
            validate_append(
                index,
                node.append.as_ref().expect("kind append implies args"),
                errors,
            );
        }
        TransformKind::Patch => {
            validate_patch(
                index,
                node.patch.as_ref().expect("kind patch implies args"),
                errors,
            );
        }
        TransformKind::Metadata => {
            validate_metadata(
                index,
                node.metadata.as_ref().expect("kind metadata implies args"),
                errors,
            );
        }
        TransformKind::CommitMessage => {
            validate_commit_message(
                index,
                node.commit_message
                    .as_ref()
                    .expect("kind commit_message implies args"),
                errors,
            );
        }
        TransformKind::AuthorMapping => {
            validate_author_mapping(
                index,
                node.author_mapping
                    .as_ref()
                    .expect("kind author_mapping implies args"),
                errors,
            );
        }
        TransformKind::DependencyRewrite => {
            validate_dependency_rewrite(
                index,
                node.dependency_rewrite
                    .as_ref()
                    .expect("kind dependency_rewrite implies args"),
                errors,
            );
        }
        TransformKind::ImportRewrite => {
            validate_import_rewrite(
                index,
                node.import_rewrite
                    .as_ref()
                    .expect("kind import_rewrite implies args"),
                errors,
            );
        }
        TransformKind::Custom => {
            validate_custom(
                index,
                node.custom.as_ref().expect("kind custom implies args"),
                errors,
            );
        }
    }
}

fn validate_filter(index: usize, args: &FilterArgs, errors: &mut Vec<ValidationError>) {
    check_non_empty_list(
        &args.paths,
        &format!("transform[{index}]: filter.paths"),
        errors,
    );
}

fn validate_delete(index: usize, args: &DeleteArgs, errors: &mut Vec<ValidationError>) {
    check_non_empty_list(
        &args.paths,
        &format!("transform[{index}]: delete.paths"),
        errors,
    );
}

fn validate_replace(index: usize, args: &ReplaceArgs, errors: &mut Vec<ValidationError>) {
    check_non_empty(
        &args.file,
        &format!("transform[{index}]: replace.file"),
        errors,
    );
    if args.replacements.is_empty() {
        errors.push(ValidationError::new(format!(
            "transform[{index}]: replace.replacements must not be empty"
        )));
    }
}

fn validate_regex_replace(
    index: usize,
    args: &RegexReplaceArgs,
    errors: &mut Vec<ValidationError>,
) {
    check_non_empty_list(
        &args.files,
        &format!("transform[{index}]: regex_replace.files"),
        errors,
    );
    check_non_empty(
        &args.pattern,
        &format!("transform[{index}]: regex_replace.pattern"),
        errors,
    );
}

fn validate_strip_prefix(index: usize, args: &StripPrefixArgs, errors: &mut Vec<ValidationError>) {
    check_non_empty(
        &args.path,
        &format!("transform[{index}]: strip_prefix.path"),
        errors,
    );
}

fn validate_prepend(index: usize, args: &PrependArgs, errors: &mut Vec<ValidationError>) {
    check_non_empty_list(
        &args.files,
        &format!("transform[{index}]: prepend.files"),
        errors,
    );
    check_non_empty(
        &args.content,
        &format!("transform[{index}]: prepend.content"),
        errors,
    );
}

fn validate_append(index: usize, args: &AppendArgs, errors: &mut Vec<ValidationError>) {
    check_non_empty_list(
        &args.files,
        &format!("transform[{index}]: append.files"),
        errors,
    );
    check_non_empty(
        &args.content,
        &format!("transform[{index}]: append.content"),
        errors,
    );
}

fn validate_patch(index: usize, args: &PatchArgs, errors: &mut Vec<ValidationError>) {
    check_non_empty(
        &args.file,
        &format!("transform[{index}]: patch.file"),
        errors,
    );
    check_non_empty(
        &args.patch,
        &format!("transform[{index}]: patch.patch"),
        errors,
    );
}

fn validate_metadata(index: usize, args: &MetadataArgs, errors: &mut Vec<ValidationError>) {
    check_non_empty(
        &args.key,
        &format!("transform[{index}]: metadata.key"),
        errors,
    );
}

fn validate_commit_message(
    index: usize,
    args: &CommitMessageArgs,
    errors: &mut Vec<ValidationError>,
) {
    check_non_empty(
        &args.message,
        &format!("transform[{index}]: commit_message.message"),
        errors,
    );
}

fn validate_author_mapping(
    index: usize,
    args: &AuthorMappingArgs,
    errors: &mut Vec<ValidationError>,
) {
    if args.mapping.is_empty() {
        errors.push(ValidationError::new(format!(
            "transform[{index}]: author_mapping.mapping must not be empty"
        )));
    }
}

fn validate_custom(index: usize, args: &CustomArgs, errors: &mut Vec<ValidationError>) {
    check_non_empty(
        &args.name,
        &format!("transform[{index}]: custom.name"),
        errors,
    );
    check_non_empty(
        &args.path,
        &format!("transform[{index}]: custom.path"),
        errors,
    );
}

fn validate_dependency_rewrite(
    index: usize,
    args: &DependencyRewriteArgs,
    errors: &mut Vec<ValidationError>,
) {
    if args.package_rename.is_empty() && args.workspace_version.is_none() {
        errors.push(ValidationError::new(format!(
            "transform[{index}]: dependency_rewrite must set package_rename and/or workspace_version"
        )));
    }
}

fn validate_import_rewrite(
    index: usize,
    args: &ImportRewriteArgs,
    errors: &mut Vec<ValidationError>,
) {
    if args.renames.is_empty() {
        errors.push(ValidationError::new(format!(
            "transform[{index}]: import_rewrite.renames must not be empty"
        )));
    }
}

fn validate_from_to(
    index: usize,
    transform: &str,
    from: &str,
    to: &str,
    allow_root: bool,
    errors: &mut Vec<ValidationError>,
) {
    let label = format!("transform[{index}]: {transform}");
    check_path_prefix(from, false, &format!("{label}.from"), errors);
    check_path_prefix(to, allow_root, &format!("{label}.to"), errors);
    if !from.is_empty() && from == to {
        errors.push(ValidationError::new(format!(
            "{label}.from must not equal {label}.to"
        )));
    }
}

fn check_path_prefix(
    value: &str,
    allow_root: bool,
    label: &str,
    errors: &mut Vec<ValidationError>,
) {
    if value.is_empty() {
        errors.push(ValidationError::new(format!("{label} must not be empty")));
        return;
    }
    if value.starts_with('/') {
        errors.push(ValidationError::new(format!(
            "{label} must be a relative path"
        )));
        return;
    }
    for segment in value.split('/') {
        if segment == ".." {
            errors.push(ValidationError::new(format!(
                "{label} must not contain '..'"
            )));
            return;
        }
        if segment == "." && !(allow_root && value == ".") {
            errors.push(ValidationError::new(format!(
                "{label} must not contain '.'"
            )));
            return;
        }
    }
}

fn check_non_empty(value: &str, label: &str, errors: &mut Vec<ValidationError>) {
    if value.trim().is_empty() {
        errors.push(ValidationError::new(format!("{label} must not be empty")));
    }
}

fn check_non_empty_list(list: &[String], label: &str, errors: &mut Vec<ValidationError>) {
    if list.is_empty() {
        errors.push(ValidationError::new(format!("{label} must not be empty")));
        return;
    }
    for (i, item) in list.iter().enumerate() {
        if item.trim().is_empty() {
            errors.push(ValidationError::new(format!(
                "{label}[{i}] must not be empty"
            )));
        }
    }
}

fn validate_policy(policy: &Policy, errors: &mut Vec<ValidationError>) {
    check_list_entries(&policy.deny, "policy.deny", errors);
    check_list_entries(&policy.require_review, "policy.require_review", errors);
}

fn check_list_entries(list: &[String], label: &str, errors: &mut Vec<ValidationError>) {
    for (i, item) in list.iter().enumerate() {
        if item.trim().is_empty() {
            errors.push(ValidationError::new(format!(
                "{label}[{i}] must not be empty"
            )));
        }
    }
}
