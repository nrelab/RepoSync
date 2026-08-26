//! Transformation engine for RepoSync.
//!
//! A [`Transformation`] rewrites an in-memory [`RepositorySnapshot`] in place.
//! Transforms compose in order via [`Runner`], which times each step and stops
//! at the first error. Because transforms only touch the snapshot they are
//! handed, every run is dry-run safe — nothing is written to disk or git.
//!
//! Tier-1 transforms (PLAN.md M4): [`Filter`], [`Delete`], [`Copy`],
//! [`Move`], [`Rename`]. Path selection uses the shared [`Glob`] matcher
//! (`*`, `?`, `**`).
//!
//! M13 string/metadata transforms: [`Replace`], [`StripPrefix`],
//! [`DependencyRewrite`], [`ImportRewrite`], [`RegexReplace`], [`Prepend`],
//! [`Append`], [`Patch`], [`Metadata`].

mod append;
#[cfg(test)]
mod test_utils;
mod context;
mod custom;
mod delete;
mod dependency;
mod error;
mod filter;
mod glob;
mod import_rewrite;
mod metadata;
mod patch;
mod prepend;
mod regex_replace;
mod relocate;
mod replace;
mod runner;
mod strip_prefix;
mod traits;

pub use append::Append;
pub use context::{TransformContext, TransformEvent, TransformResult};
pub use custom::PluginTransform;
pub use delete::Delete;
pub use dependency::DependencyRewrite;
pub use error::Error;
pub use filter::Filter;
pub use glob::Glob;
pub use import_rewrite::ImportRewrite;
pub use metadata::Metadata;
pub use patch::Patch;
pub use prepend::Prepend;
pub use regex_replace::RegexReplace;
pub use relocate::{Copy, Move, Rename};
pub use replace::Replace;
pub use runner::{RunReport, Runner, StepReport};
pub use strip_prefix::StripPrefix;
pub use traits::Transformation;
