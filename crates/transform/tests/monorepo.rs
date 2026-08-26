//! M13 DoD fixture: extract a monorepo package and rewrite its imports and
//! dependency versions. Mirrors the PLAN.md scenario: filter a package, strip
//! its prefix, rewrite `@internal/*` module specifiers and pin `workspace:*`
//! dependencies to a concrete public release.

use std::collections::BTreeMap;

use reposync_core::{Blob, FileEntry, RepoPath, RepositorySnapshot};
use reposync_transform::{
    DependencyRewrite, Filter, ImportRewrite, Runner, StripPrefix, Transformation,
};

fn monorepo() -> RepositorySnapshot {
    let mut snapshot = RepositorySnapshot::new();

    // The `sdk` package we intend to extract.
    snapshot.insert(FileEntry::regular(
        RepoPath::new("packages/sdk/package.json").unwrap(),
        Blob::from_bytes(
            br#"{
  "name": "@internal/sdk",
  "dependencies": { "@internal/cli": "workspace:*" }
}"#,
        ),
    ));
    snapshot.insert(FileEntry::regular(
        RepoPath::new("packages/sdk/src/index.ts").unwrap(),
        Blob::from_bytes(
            b"import { cli } from '@internal/cli';\nimport { sdk } from '@internal/sdk';\n",
        ),
    ));

    // A sibling package that stays behind in the monorepo.
    snapshot.insert(FileEntry::regular(
        RepoPath::new("packages/cli/package.json").unwrap(),
        Blob::from_bytes(br#"{ "name": "@internal/cli" }"#),
    ));
    snapshot.insert(FileEntry::regular(
        RepoPath::new("packages/cli/src/index.ts").unwrap(),
        Blob::from_bytes(b"import { sdk } from '@internal/sdk';\n"),
    ));

    snapshot
}

fn build_pipeline() -> Vec<Box<dyn Transformation>> {
    let mut rename = BTreeMap::new();
    rename.insert("@internal/sdk".to_owned(), "@public/sdk".to_owned());
    rename.insert("@internal/cli".to_owned(), "@public/cli".to_owned());

    vec![
        Box::new(Filter::new(["packages/sdk/**"])),
        Box::new(StripPrefix::new("packages/sdk")),
        Box::new(DependencyRewrite::new(
            rename.clone(),
            Some("2.3.4".to_owned()),
        )),
        Box::new(ImportRewrite::new(rename)),
    ]
}

#[test]
fn extracts_package_with_rewritten_imports_and_dependency_versions() {
    let snapshot = monorepo();
    let pipeline = build_pipeline();
    let refs: Vec<&dyn Transformation> = pipeline.iter().map(|b| &**b).collect();

    let report = Runner::new().run(snapshot, &refs).unwrap();
    let out = &report.snapshot;

    // The sibling package was filtered out; only the extracted package remains.
    assert!(out.contains(&RepoPath::new("src/index.ts").unwrap()));
    assert!(!out.contains(&RepoPath::new("packages/cli/src/index.ts").unwrap()));
    assert!(out.contains(&RepoPath::new("package.json").unwrap()));

    // Imports were rewritten from @internal/* to @public/* (including subpath).
    let src = out.get(&RepoPath::new("src/index.ts").unwrap()).unwrap();
    let src_text = std::str::from_utf8(src.content.content()).unwrap();
    assert!(src_text.contains("import { cli } from '@public/cli';"));
    assert!(src_text.contains("import { sdk } from '@public/sdk';"));
    assert!(!src_text.contains("@internal/"));

    // The package manifest: name renamed and workspace dependency pinned.
    let manifest = out.get(&RepoPath::new("package.json").unwrap()).unwrap();
    let doc: serde_json::Value =
        serde_json::from_slice(manifest.content.content()).expect("valid json");
    assert_eq!(doc["name"], serde_json::json!("@public/sdk"));
    assert_eq!(
        doc["dependencies"]["@public/cli"],
        serde_json::json!("2.3.4")
    );
}
