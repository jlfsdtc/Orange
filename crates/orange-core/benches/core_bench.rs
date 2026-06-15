//! Benchmarks for orange-core: indexing, line storage, and search.

use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;
use orange_core::log_data::LogData;
use orange_core::log_filtered_data::LogFilteredData;
use orange_regex::RegexFlags;
use std::io::Write;
use tempfile::NamedTempFile;

fn create_log_file(num_lines: usize) -> NamedTempFile {
    let mut file = NamedTempFile::new().unwrap();
    for i in 0..num_lines {
        writeln!(file, "2024-01-15 10:30:00.{:06} INFO [thread-{}] processing item {} with data payload",
            i % 1_000_000, i % 16, i).unwrap();
    }
    file.flush().unwrap();
    file
}

fn bench_indexing(c: &mut Criterion) {
    let mut group = c.benchmark_group("indexing");

    let file_1k = create_log_file(1_000);
    let file_100k = create_log_file(100_000);

    group.bench_function("1k_lines", |b| {
        b.iter(|| LogData::open(black_box(file_1k.path())).unwrap());
    });

    group.bench_function("100k_lines", |b| {
        b.iter(|| LogData::open(black_box(file_100k.path())).unwrap());
    });

    group.finish();
}

fn bench_line_access(c: &mut Criterion) {
    let mut group = c.benchmark_group("line_access");

    let file = create_log_file(100_000);
    let data = LogData::open(file.path()).unwrap();

    group.bench_function("get_line_sequential", |b| {
        let mut i = 0u64;
        b.iter(|| {
            let line = data.get_line(black_box(i));
            i = (i + 1) % data.line_count();
            line
        });
    });

    group.bench_function("get_lines_batch_100", |b| {
        let mut start = 0u64;
        b.iter(|| {
            let lines = data.get_lines(black_box(start), 100);
            start = (start + 100) % (data.line_count() - 100);
            lines
        });
    });

    group.finish();
}

fn bench_search(c: &mut Criterion) {
    let mut group = c.benchmark_group("search");

    let file = create_log_file(10_000);
    let data = LogData::open(file.path()).unwrap();

    group.bench_function("literal_10k_lines", |b| {
        b.iter(|| {
            let mut filtered = LogFilteredData::new();
            filtered.search(black_box(&data), "ERROR", RegexFlags::default()).unwrap();
            filtered
        });
    });

    group.bench_function("regex_10k_lines", |b| {
        b.iter(|| {
            let mut filtered = LogFilteredData::new();
            filtered.search(black_box(&data), r"\d{4}-\d{2}-\d{2}", RegexFlags::default()).unwrap();
            filtered
        });
    });

    group.finish();
}

criterion_group!(benches, bench_indexing, bench_line_access, bench_search);
criterion_main!(benches);
