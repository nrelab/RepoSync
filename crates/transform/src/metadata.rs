use crate::{Error, TransformContext, TransformEvent, TransformResult, Transformation};

/// Sets a user-defined key/value pair in the snapshot's repository metadata.
#[derive(Debug, Clone)]
pub struct Metadata {
    key: String,
    value: String,
}

impl Metadata {
    /// Creates a new [`Metadata`] transform.
    #[must_use]
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
        }
    }
}

impl Transformation for Metadata {
    fn name(&self) -> &'static str {
        "metadata"
    }

    fn apply(&self, ctx: &mut TransformContext) -> Result<TransformResult, Error> {
        ctx.snapshot
            .metadata_mut()
            .custom
            .insert(self.key.clone(), self.value.clone());
        Ok(TransformResult {
            changed: 0,
            warnings: Vec::new(),
            event: TransformEvent::Rewrote { files: 0 },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::snapshot_from;

    #[test]
    fn sets_custom_metadata() {
        let mut ctx = TransformContext::new(snapshot_from(&[("a.txt", b"x")]));
        let t = Metadata::new("license", "MIT");
        t.apply(&mut ctx).unwrap();
        assert_eq!(
            ctx.snapshot.metadata.custom.get("license"),
            Some(&"MIT".to_string())
        );
    }
}
