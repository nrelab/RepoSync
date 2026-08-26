use reposync_core::{CommitId, RepositorySnapshot};
use reposync_diff::{diff, SnapshotDiff};
use reposync_git::{CommitSpec, GitRepo};
use reposync_transform::{Runner, StepReport, Transformation};

use crate::Error;

/// Outcome of a sync run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncReport {
    /// Whether the destination was modified (a commit was created).
    pub changed: bool,
    /// What will change in the destination: the diff between the transformed
    /// output and the destination's current HEAD.
    pub diff: SnapshotDiff,
    /// The commit created on the destination, if any.
    pub commit_id: Option<CommitId>,
    /// Whether the commit was pushed to the destination remote.
    pub pushed: bool,
    /// Per-step transform reports, in order.
    pub steps: Vec<StepReport>,
    /// The transformed snapshot (before commit).
    pub output: RepositorySnapshot,
}

/// Runs the sync pipeline: read source → transform → diff → commit → push.
///
/// The pipeline is dry-run safe and atomic with respect to the destination:
/// transforms run entirely in memory and the destination is only touched after
/// every transform succeeds. A run whose output matches the destination's
/// current HEAD makes zero commits.
#[derive(Debug, Clone, Copy)]
pub struct Syncer<'a> {
    source: &'a GitRepo,
    dest: &'a GitRepo,
}

impl<'a> Syncer<'a> {
    /// Creates a syncer that reads from `source` and writes to `dest`.
    #[must_use]
    pub fn new(source: &'a GitRepo, dest: &'a GitRepo) -> Self {
        Self { source, dest }
    }

    /// Runs the pipeline.
    ///
    /// Reads the source snapshot at HEAD, applies `transforms` in order, and
    /// diffs the result against the destination's current HEAD. An empty diff
    /// means the run is a no-op: nothing is committed and nothing is pushed.
    /// Otherwise the transformed snapshot is committed to `dest` and, when
    /// `push_url` is given, pushed to it.
    ///
    /// # Errors
    ///
    /// Returns the first transform or git error. On error the destination is
    /// left untouched.
    pub fn run(
        &self,
        transforms: &[&dyn Transformation],
        commit: &CommitSpec,
        push_url: Option<&str>,
    ) -> Result<SyncReport, Error> {
        let mut report = self.plan(transforms)?;
        if !report.changed {
            return Ok(report);
        }
        // Mark mirror commits so reverse sync can recognize and skip them.
        let source_head = self.source.head_commit_id()?;
        let commit_spec = match source_head {
            Some(ref head) => CommitSpec {
                message: reposync_git::mark_generated(&commit.message, head),
                author: commit.author.clone(),
                committer: commit.committer.clone(),
            },
            None => commit.clone(),
        };
        let commit_id = self.dest.write_commit(&report.output, &commit_spec)?;
        let mut pushed = false;
        if let Some(url) = push_url {
            self.dest.push(url)?;
            pushed = true;
        }
        report.commit_id = Some(commit_id);
        report.pushed = pushed;
        Ok(report)
    }

    /// Plan the pipeline without committing or pushing.
    ///
    /// Returns the transformed output and the diff against the destination's
    /// current HEAD. Use this to inspect the planned changes before applying
    /// them.
    ///
    /// # Errors
    ///
    /// Returns the first transform or git error.
    pub fn plan(&self, transforms: &[&dyn Transformation]) -> Result<SyncReport, Error> {
        let input = self.source.head_snapshot()?;
        let run_report = Runner::new().run(input, transforms)?;
        let output = run_report.snapshot;

        let dest_snapshot = self.dest.head_snapshot()?;
        let diff = diff(&dest_snapshot, &output);
        if diff.is_empty() {
            return Ok(SyncReport {
                changed: false,
                diff,
                commit_id: None,
                pushed: false,
                steps: run_report.steps,
                output,
            });
        }

        Ok(SyncReport {
            changed: true,
            diff,
            commit_id: None,
            pushed: false,
            steps: run_report.steps,
            output,
        })
    }
}
