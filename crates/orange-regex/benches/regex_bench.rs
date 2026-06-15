//! Benchmarks for orange-regex: Hyperscan pattern compilation and scanning.

use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;
use orange_regex::engine::RegexEngine;
use orange_regex::RegexFlags;

fn bench_compile(c: &mut Criterion) {
    c.bench_function("compile_simple", |b| {
        b.iter(|| RegexEngine::compile(black_box("error"), RegexFlags::default()).unwrap());
    });

    c.bench_function("compile_regex", |b| {
        b.iter(|| {
            RegexEngine::compile(
                black_box(r"\d{4}-\d{2}-\d{2}"),
                RegexFlags::default(),
            )
            .unwrap()
        });
    });
}

fn bench_scan(c: &mut Criterion) {
    let small = "2024-01-15 10:30:00 ERROR: something went wrong\n".repeat(10);
    let medium = "2024-01-15 10:30:00 INFO: normal operation\n".repeat(1000);

    let engine = RegexEngine::compile("ERROR", RegexFlags::default()).unwrap();

    c.bench_function("scan_10_lines", |b| {
        b.iter(|| engine.scan(black_box(small.as_bytes())).unwrap());
    });

    c.bench_function("scan_1000_lines", |b| {
        b.iter(|| engine.scan(black_box(medium.as_bytes())).unwrap());
    });

    let engine_re = RegexEngine::compile(r"\d{4}-\d{2}-\d{2}", RegexFlags::default()).unwrap();

    c.bench_function("scan_regex_1000_lines", |b| {
        b.iter(|| engine_re.scan(black_box(medium.as_bytes())).unwrap());
    });
}

fn bench_scan_first(c: &mut Criterion) {
    let no_match = "2024-01-15 10:30:00 INFO: normal\n".repeat(1000);
    let engine = RegexEngine::compile("ERROR", RegexFlags::default()).unwrap();

    c.bench_function("scan_first_no_match_1k", |b| {
        b.iter(|| engine.scan_first(black_box(no_match.as_bytes())).unwrap());
    });
}

criterion_group!(benches, bench_compile, bench_scan, bench_scan_first);
criterion_main!(benches);
