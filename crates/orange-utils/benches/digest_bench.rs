//! Benchmarks for orange-utils: file digest computation.

use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;
use orange_utils::file_digest::FileDigest;
use std::io::Write;
use tempfile::NamedTempFile;

fn create_temp_file(size: usize) -> NamedTempFile {
    let mut file = NamedTempFile::new().unwrap();
    let data = vec![0u8; size];
    file.write_all(&data).unwrap();
    file.flush().unwrap();
    file
}

fn bench_file_digest(c: &mut Criterion) {
    let file_1k = create_temp_file(1024);
    let file_1m = create_temp_file(1024 * 1024);

    c.bench_function("digest_1kb", |b| {
        b.iter(|| FileDigest::compute(black_box(file_1k.path())).unwrap());
    });

    c.bench_function("digest_1mb", |b| {
        b.iter(|| FileDigest::compute(black_box(file_1m.path())).unwrap());
    });
}

criterion_group!(benches, bench_file_digest);
criterion_main!(benches);
