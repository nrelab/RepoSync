//! M14 integration test: a WASM plugin (authored in WAT, no external toolchain)
//! echoes its input envelope, proving the full host<->guest ABI round-trip:
//! serialize `PluginInput` -> WASM `transform` -> deserialize snapshot.

use std::collections::BTreeMap;

use reposync_core::{Blob, FileEntry, RepoPath, RepositorySnapshot};
use reposync_transform::{PluginTransform, Transformation, TransformContext};

/// Identity guest: exports `memory`, `alloc`, and `transform`. `transform`
/// allocates `[u32 len][bytes]` and copies the input verbatim (a correct
/// identity transform). A real plugin would parse the JSON and rewrite it.
const IDENTITY_GUEST: &str = r#"
(module
  (memory (export "memory") 4)
  (global $next (mut i32) (i32.const 1024))
  (func $alloc (export "alloc") (param $size i32) (result i32)
    (local $old i32)
    (local.set $old (global.get $next))
    (global.set $next
      (i32.and
        (i32.add (global.get $next) (i32.add (local.get $size) (i32.const 15)))
        (i32.const -16)))
    (local.get $old))
  (func (export "transform") (param $ptr i32) (param $len i32) (result i32)
    (local $out i32)
    (local.set $out (call $alloc (i32.add (local.get $len) (i32.const 4))))
    (i32.store (local.get $out) (local.get $len))
    (memory.copy (i32.add (local.get $out) (i32.const 4)) (local.get $ptr) (local.get $len))
    (local.get $out))
)
"#;

/// Guest that exports `memory` + `alloc` but no `transform` (must be rejected).
const INCOMPLETE_GUEST: &str = r#"
(module
  (memory (export "memory") 1)
  (func (export "alloc") (param $size i32) (result i32) (local.get $size))
)
"#;

fn snapshot() -> RepositorySnapshot {
    let mut s = RepositorySnapshot::new();
    s.insert(FileEntry::regular(
        RepoPath::new("a.txt").unwrap(),
        Blob::from_bytes(b"hello"),
    ));
    s.insert(FileEntry::regular(
        RepoPath::new("b/c.txt").unwrap(),
        Blob::from_bytes(b"world"),
    ));
    s
}

#[test]
fn wasm_plugin_round_trips_snapshot() {
    let wasm = wat::parse_str(IDENTITY_GUEST).expect("compile wat");
    let plugin = PluginTransform::from_wasm_bytes("identity", &wasm, &BTreeMap::new()).unwrap();

    let snap = snapshot();
    let mut ctx = TransformContext::new(snap.clone());
    let result = plugin.apply(&mut ctx).unwrap();

    // Identity: nothing changed, and the snapshot is preserved exactly.
    assert_eq!(result.changed, 0);
    assert_eq!(ctx.snapshot, snap);
}

#[test]
fn plugin_missing_transform_is_rejected() {
    let wasm = wat::parse_str(INCOMPLETE_GUEST).expect("compile wat");
    let error = PluginTransform::from_wasm_bytes("bad", &wasm, &BTreeMap::new()).unwrap_err();
    assert!(
        format!("{error}").contains("transform"),
        "expected a missing-symbol error, got: {error}"
    );
}
