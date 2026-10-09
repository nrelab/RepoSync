use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use reposync_core::{Blob, FileEntry, FileMode, RepoPath, RepositorySnapshot};

fn bench_blob_hash(c: &mut Criterion) {
    let content = b"Hello, World! This is a test content for hashing benchmarks.";
    let blob = Blob::new(content.to_vec());

    c.bench_function("blob_hash", |b| {
        b.iter(|| black_box(blob.hash()));
    });
}

fn bench_blob_hash_large(c: &mut Criterion) {
    let content = vec![b'x'; 1024 * 1024]; // 1MB
    let blob = Blob::new(content);

    c.bench_function("blob_hash_large_1mb", |b| {
        b.iter(|| black_box(blob.hash()));
    });
}

fn bench_file_entry_create(c: &mut Criterion) {
    let path = RepoPath::new("src/lib.rs").unwrap();
    let content = Blob::new(b"fn main() {}".to_vec());
    let mode = FileMode::File;

    c.bench_function("file_entry_create", |b| {
        b.iter(|| {
            black_box(FileEntry::new(
                black_box(path.clone()),
                black_box(content.clone()),
                black_box(mode),
            ))
        });
    });
}

fn bench_snapshot_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("snapshot_insert");

    #[allow(clippy::explicit_iter_loop)]
    for size in [10, 100, 1000, 10000].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            let mut snapshot = RepositorySnapshot::default();
            let base_path = "src/";
            let content = Blob::new(b"test content".to_vec());
            let mode = FileMode::File;

            #[allow(clippy::uninlined_format_args)]
            b.iter(|| {
                for i in 0..size {
                    let path = RepoPath::new(format!("{base_path}file_{i}.rs")).unwrap();
                    snapshot.insert(FileEntry::new(path, content.clone(), mode));
                }
            });
        });
    }

    group.finish();
}

fn bench_snapshot_serialization(c: &mut Criterion) {
    let mut snapshot = RepositorySnapshot::default();
    #[allow(clippy::uninlined_format_args)]
    for i in 0..1000 {
        let path = RepoPath::new(format!("src/file_{i}.rs")).unwrap();
        let content = Blob::new(format!("fn test_{i}() {{}}").into_bytes());
        let mode = FileMode::File;
        snapshot.insert(FileEntry::new(path, content, mode));
    }

    c.bench_function("snapshot_serialize_json", |b| {
        b.iter(|| black_box(serde_json::to_string(&snapshot)));
    });

    c.bench_function("snapshot_deserialize_json", |b| {
        let json = serde_json::to_string(&snapshot).unwrap();
        b.iter(|| black_box(serde_json::from_str::<RepositorySnapshot>(&json)));
    });
}

fn bench_path_validation(c: &mut Criterion) {
    let valid_paths = ["src/lib.rs", "packages/sdk/index.ts", "README.md"];
    let invalid_paths = ["/absolute/path.rs", "../parent.rs", "path/../escape.rs"];

    c.bench_function("path_validation_valid", |b| {
        b.iter(|| {
            for path in valid_paths {
                let _ = black_box(RepoPath::new(path));
            }
        });
    });

    c.bench_function("path_validation_invalid", |b| {
        b.iter(|| {
            for path in invalid_paths {
                let _ = black_box(RepoPath::new(path));
            }
        });
    });
}

criterion_group!(
    benches,
    bench_blob_hash,
    bench_blob_hash_large,
    bench_file_entry_create,
    bench_snapshot_insert,
    bench_snapshot_serialization,
    bench_path_validation
);
criterion_main!(benches);
