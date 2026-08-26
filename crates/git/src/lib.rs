//! Git backend for RepoSync.
//!
//! [`GitRepo`] wraps a `libgit2` (`git2`) repository and exchanges data with
//! the engine exclusively through [`RepositorySnapshot`] values:
//!
//! - read the tree at HEAD (or any ref) into a snapshot — [`GitRepo::head_snapshot`]
//! - write a snapshot as a new commit — [`GitRepo::write_commit`]
//! - fetch, checkout, and push — [`GitRepo::fetch`], [`GitRepo::checkout`], [`GitRepo::push`]
//!
//! The snapshot ⇄ tree conversions preserve file modes (regular, executable,
//! symlink, gitlink). The engine never manipulates a working tree directly;
//! `GitRepo` is its boundary into git object storage.

mod error;
mod marker;
mod repo;

pub use error::Error;
pub use marker::{generated_source, mark_generated, GENERATED_TRAILER};
pub use repo::{CommitSpec, GitRepo};
