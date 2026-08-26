use std::time::{Duration, Instant};

use reposync_core::RepositorySnapshot;

use crate::{Error, TransformContext, TransformEvent, Transformation};

/// A sequential, timed, fail-fast pipeline runner.
///
/// Transforms run in the order given; the first error aborts the run.
#[derive(Debug, Clone, Copy, Default)]
pub struct Runner {
    dry_run: bool,
}

impl Runner {
    /// Creates a runner (materializing by default).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Marks the run as a dry run. Transforms never touch disk or git, so this
    /// only affects reporting bookkeeping.
    #[must_use]
    pub fn dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }

    /// Applies `transforms` to `snapshot` in order.
    ///
    /// # Errors
    ///
    /// Returns the first transform error and stops the run.
    pub fn run(
        &self,
        snapshot: RepositorySnapshot,
        transforms: &[&dyn Transformation],
    ) -> Result<RunReport, Error> {
        let mut ctx = TransformContext::new(snapshot).dry_run(self.dry_run);
        let mut steps = Vec::new();
        for transform in transforms {
            let start = Instant::now();
            let result = transform.apply(&mut ctx)?;
            steps.push(StepReport {
                name: transform.name().to_owned(),
                changed: result.changed,
                duration: start.elapsed(),
                warnings: result.warnings.clone(),
            });
            ctx.warnings.extend(result.warnings);
            ctx.events.push(result.event);
        }
        Ok(RunReport {
            snapshot: ctx.snapshot,
            steps,
            warnings: ctx.warnings,
            events: ctx.events,
        })
    }
}

/// One step's timing and outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepReport {
    /// The transform's stable name.
    pub name: String,
    /// Number of files changed by this step.
    pub changed: usize,
    /// Wall-clock time the step took.
    pub duration: Duration,
    /// Warnings raised by this step.
    pub warnings: Vec<String>,
}

/// Full outcome of a pipeline run.
#[derive(Debug, Clone)]
pub struct RunReport {
    /// The snapshot after all transforms.
    pub snapshot: RepositorySnapshot,
    /// Per-step reports, in order.
    pub steps: Vec<StepReport>,
    /// All warnings, across the pipeline.
    pub warnings: Vec<String>,
    /// All structured events, across the pipeline.
    pub events: Vec<TransformEvent>,
}
