use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use tempfile::TempDir;

// Import the crate functions we want to benchmark
use magento_static_deploy::copier::{copy_directory_with_overrides, copy_file};
use magento_static_deploy::scanner::discover_themes;
use magento_static_deploy::theme::Area;

/// Create a test directory structure with N files
fn create_test_files(dir: &TempDir, count: usize) -> PathBuf {
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();

    for i in 0..count {
        let subdir = src.join(format!("dir{}", i % 10));
        fs::create_dir_all(&subdir).unwrap();
        let file = subdir.join(format!("file{}.txt", i));
        fs::write(&file, format!("content {}", i)).unwrap();
    }

    src
}

/// Benchmark file copy operations
fn bench_copy_file(c: &mut Criterion) {
    let temp = TempDir::new().unwrap();
    let src = temp.path().join("source.txt");
    let dst = temp.path().join("dest.txt");

    // Create a 1KB file
    fs::write(&src, vec![b'x'; 1024]).unwrap();

    c.bench_function("copy_file_1kb", |b| {
        b.iter(|| {
            let _ = fs::remove_file(&dst);
            copy_file(black_box(&src), black_box(&dst)).unwrap()
        })
    });

    // Create a 1MB file
    fs::write(&src, vec![b'x'; 1024 * 1024]).unwrap();

    c.bench_function("copy_file_1mb", |b| {
        b.iter(|| {
            let _ = fs::remove_file(&dst);
            copy_file(black_box(&src), black_box(&dst)).unwrap()
        })
    });
}

/// Benchmark directory copy with different file counts
fn bench_copy_directory(c: &mut Criterion) {
    let mut group = c.benchmark_group("copy_directory");
    let shutdown = AtomicBool::new(false);

    for file_count in [100, 500, 1000].iter() {
        let temp = TempDir::new().unwrap();
        let src = create_test_files(&temp, *file_count);
        let dst = temp.path().join("dst");

        group.throughput(Throughput::Elements(*file_count as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(file_count),
            file_count,
            |b, _| {
                b.iter(|| {
                    let _ = fs::remove_dir_all(&dst);
                    fs::create_dir_all(&dst).unwrap();
                    copy_directory_with_overrides(black_box(&src), black_box(&dst), &shutdown)
                        .unwrap()
                })
            },
        );
    }

    group.finish();
}

/// Benchmark theme discovery (requires mock Magento structure)
fn bench_theme_discovery(c: &mut Criterion) {
    let temp = TempDir::new().unwrap();

    // Create mock Magento app/design structure
    let design = temp.path().join("app/design/frontend");
    fs::create_dir_all(&design).unwrap();

    // Create 5 mock themes
    for i in 0..5 {
        let theme_dir = design.join(format!("Vendor{}/theme{}", i, i));
        fs::create_dir_all(&theme_dir).unwrap();

        let theme_xml = theme_dir.join("theme.xml");
        fs::write(
            &theme_xml,
            r#"<?xml version="1.0"?>
<theme xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
    <title>Test Theme</title>
</theme>"#,
        )
        .unwrap();
    }

    c.bench_function("discover_themes_5", |b| {
        b.iter(|| discover_themes(black_box(temp.path()), black_box(Area::Frontend)))
    });
}

criterion_group!(
    benches,
    bench_copy_file,
    bench_copy_directory,
    bench_theme_discovery,
);
criterion_main!(benches);
