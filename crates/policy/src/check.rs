use reposync_config::Policy;
use reposync_core::{RepoPath, RepositorySnapshot};
use reposync_diff::SnapshotDiff;
use reposync_transform::Glob;

/// Outcome of a policy check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyReport {
    pub denied: Vec<PolicyViolation>,
    pub review: Vec<PolicyViolation>,
    pub deleted_count: u64,
    pub allowed: bool,
}

/// A single policy violation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyViolation {
    pub path: RepoPath,
    pub pattern: String,
}

impl PolicyReport {
    #[must_use]
    pub fn is_allowed(&self) -> bool {
        self.allowed && self.review.is_empty()
    }
}

/// Evaluate `policy` against a planned `diff` and `output` snapshot.
///
/// Returns a `PolicyReport` describing any violations and whether the run is
/// allowed to proceed.
#[must_use]
pub fn check(policy: &Policy, diff: &SnapshotDiff, output: &RepositorySnapshot) -> PolicyReport {
    let denied = check_globs(&policy.deny, output);
    let review = check_globs(&policy.require_review, output);
    let deleted_count = diff.removed.len() as u64;
    let allowed = denied.is_empty() && deleted_count <= policy.max_deleted_files.unwrap_or(u64::MAX);
    PolicyReport { denied, review, deleted_count, allowed }
}

fn check_globs(patterns: &[String], output: &RepositorySnapshot) -> Vec<PolicyViolation> {
    let mut violations = Vec::new();
    for pattern in patterns {
        let Ok(glob) = Glob::new(pattern) else {
            continue;
        };
        for path in output.paths() {
            let path: &RepoPath = path;
            if reposync_transform::Glob::matches(&glob, path) {
                violations.push(PolicyViolation {
                    path: path.clone(),
                    pattern: pattern.clone(),
                });
            }
        }
    }
    violations
}
