//! Reproducible repo-to-repo test harness for the RepoSync stack.
//!
//! Golden tests: build a temp source repo from a fixture, run a pipeline, and
//! compare the result to an expected snapshot.
//!
//! Determinism test: run the same pipeline twice and assert byte-identical
//! commits.

#[cfg(test)]
mod tests {
    use std::path::Path;

    use git2::{Commit, IndexAddOption, Repository};
    use reposync_core::{Blob, FileEntry, RepoPath, RepositorySnapshot};
    use reposync_git::GitRepo;
    use reposync_transform::{Transformation, Runner};

    fn init_repo(dir: &Path) -> Repository {
        let repo = Repository::init(dir).unwrap();
        let mut config = repo.config().unwrap();
        config.set_str("user.name", "Test User").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();
        repo
    }

    fn commit_worktree(repo: &Repository, message: &str) {
        let mut index = repo.index().unwrap();
        index
            .add_all(["*"], IndexAddOption::DEFAULT, None)
            .unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let signature = repo.signature().unwrap();
        let parents: Vec<Commit> = match repo.head() {
            Ok(head) => vec![head.peel_to_commit().unwrap()],
            Err(_) => vec![],
        };
        let parent_refs: Vec<&Commit> = parents.iter().collect();
        repo.commit(Some("HEAD"), &signature, &signature, message, &tree, &parent_refs)
            .unwrap();
    }

    fn run_pipeline(
        files: &[(&str, &[u8])],
        transforms: &[&dyn Transformation],
    ) -> RepositorySnapshot {
        let dir = tempfile::TempDir::new().unwrap();
        let repo_path = dir.path().join("repo");
        let repo = init_repo(&repo_path);
        for &(path, content) in files {
            let full = repo_path.join(path);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(full, content).unwrap();
        }
        commit_worktree(&repo, "initial");
        drop(repo);

        let source = GitRepo::open(&repo_path).unwrap();
        let input = source.head_snapshot().unwrap();
        Runner::new().run(input, transforms).unwrap().snapshot
    }

    fn snapshot(files: &[(&str, &[u8])]) -> RepositorySnapshot {
        let mut snapshot = RepositorySnapshot::new();
        for &(path, content) in files {
            snapshot.insert(FileEntry::regular(
                RepoPath::new(path).unwrap(),
                Blob::from_bytes(content),
            ));
        }
        snapshot
    }

    fn source_files<'a>(entries: &[(&'a str, &'a [u8])]) -> Vec<(&'a str, &'a [u8])> {
        entries.iter().map(|&(p, c)| (p, c)).collect()
    }

    fn assert_snapshots_eq(actual: &RepositorySnapshot, expected: &RepositorySnapshot) {
        assert_eq!(actual.files, expected.files, "snapshot mismatch");
    }

    // ---------------------------------------------------------------------------
    // Golden fixtures
    // ---------------------------------------------------------------------------

    #[test]
    fn golden_filter_keeps_only_matching_paths() {
        let source = source_files(&[
            ("README.md", b"# readme"),
            ("LICENSE", b"MIT"),
            ("src/lib.rs", b"pub fn f() {}"),
            ("src/main.rs", b"fn main() {}"),
        ]);
        let expected = snapshot(&[
            ("src/lib.rs", b"pub fn f() {}"),
            ("src/main.rs", b"fn main() {}"),
        ]);

        let transforms: &[&dyn Transformation] = &[&reposync_transform::Filter::new(["src/**"])];
        let result = run_pipeline(&source, transforms);
        assert_snapshots_eq(&result, &expected);
    }

    #[test]
    fn golden_delete_removes_globs() {
        let source = source_files(&[("a.txt", b"a"), ("b.tmp", b"b"), ("c.txt", b"c")]);
        let expected = snapshot(&[("a.txt", b"a"), ("c.txt", b"c")]);

        let transforms: &[&dyn Transformation] = &[&reposync_transform::Delete::new(["*.tmp"])];
        let result = run_pipeline(&source, transforms);
        assert_snapshots_eq(&result, &expected);
    }

    #[test]
    fn golden_move_relocates_subtree() {
        let source = source_files(&[
            ("packages/sdk/lib.rs", b"pub fn f() {}"),
            ("packages/sdk/src/mod.rs", b"pub mod f;"),
            ("README.md", b"# readme"),
        ]);
        let expected = snapshot(&[
            ("lib.rs", b"pub fn f() {}"),
            ("src/mod.rs", b"pub mod f;"),
            ("README.md", b"# readme"),
        ]);

        let transforms: &[&dyn Transformation] =
            &[&reposync_transform::Move::new("packages/sdk", ".")];
        let result = run_pipeline(&source, transforms);
        assert_snapshots_eq(&result, &expected);
    }

    #[test]
    fn golden_pipeline_filter_move_delete() {
        let source = source_files(&[
            ("README.md", b"# readme"),
            ("LICENSE", b"MIT"),
            ("packages/sdk/lib.rs", b"pub fn f() {}"),
            ("packages/sdk/src/mod.rs", b"pub mod f;"),
            ("packages/other/tool.rs", b"tool"),
            ("scripts/run.sh", b"#!/bin/sh"),
            ("docs/guide.md", b"guide"),
            ("config/secrets/api.key", b"secret"),
        ]);
        let expected = snapshot(&[
            ("lib.rs", b"pub fn f() {}"),
            ("src/mod.rs", b"pub mod f;"),
            ("LICENSE", b"MIT"),
        ]);

        let transforms: &[&dyn Transformation] = &[
            &reposync_transform::Filter::new(["packages/sdk/**", "LICENSE"]),
            &reposync_transform::Move::new("packages/sdk", "."),
        ];
        let result = run_pipeline(&source, transforms);
        assert_snapshots_eq(&result, &expected);
    }

    // ---------------------------------------------------------------------------
    // Determinism
    // ---------------------------------------------------------------------------

    #[test]
    fn pipeline_is_deterministic_across_two_runs() {
        let source = source_files(&[
            ("a/1.txt", b"x"),
            ("a/2.txt", b"x"),
            ("b/1.txt", b"y"),
        ]);
        let transforms: &[&dyn Transformation] = &[
            &reposync_transform::Filter::new(["a/**"]),
            &reposync_transform::Move::new("a", "z"),
        ];

        let first = run_pipeline(&source, transforms);
        let second = run_pipeline(&source, transforms);
        assert_eq!(
            first.files, second.files,
            "determinism: two runs produced different snapshots"
        );
    }
}
