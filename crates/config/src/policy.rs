use serde::{Deserialize, Serialize};

/// Safety policy applied before a pipeline pushes to its destination.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Policy {
    /// Paths (globs) that must never reach the destination.
    pub deny: Vec<String>,
    /// Paths (globs) that require review before pushing.
    pub require_review: Vec<String>,
    /// Maximum number of files a single run may delete.
    pub max_deleted_files: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_policy() {
        let policy: Policy = serde_yaml::from_str(
            r#"
deny:
  - "*.pem"
  - .env
require_review:
  - package.json
max_deleted_files: 100
"#,
        )
        .unwrap();
        assert_eq!(policy.deny.len(), 2);
        assert_eq!(policy.require_review, vec!["package.json".to_string()]);
        assert_eq!(policy.max_deleted_files, Some(100));
    }
}
