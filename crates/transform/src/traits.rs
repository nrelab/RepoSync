use crate::{Error, TransformContext, TransformResult};

/// A named snapshot rewrite applied in a fixed order by a
/// [`Runner`](crate::Runner).
pub trait Transformation {
    /// Stable name of the transform, used in reports and errors.
    fn name(&self) -> &'static str;

    /// Applies the transform to `ctx.snapshot` in place.
    ///
    /// # Errors
    ///
    /// Returns an error when the transform cannot be applied (for example a
    /// path collision). The runner stops at the first error.
    fn apply(&self, ctx: &mut TransformContext) -> Result<TransformResult, Error>;
}
