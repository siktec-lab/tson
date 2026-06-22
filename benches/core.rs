//! Criterion micro-benchmarks for the TSON hot paths.
//!
//! Run with: `cargo bench`
//!
//! Covers the full pipeline on two representative example files:
//!   compile (JSON -> TsonDocument), encode (-> bytes), decode (-> TsonDocument),
//!   decompile (-> serde_json::Value), and the full round-trip.

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::fs;
use std::hint::black_box;

const FILES: &[&str] = &["examples/telemetry.json", "examples/128KB.json"];

fn bench_pipeline(c: &mut Criterion) {
    for &path in FILES {
        let json = fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("read {path}: {e}"));
        let name = std::path::Path::new(path)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();

        // Pre-built artifacts for the per-stage benches.
        let doc = tson::compile_json(&json).unwrap();
        let bytes = tson::to_bytes(&doc).unwrap();

        let mut g = c.benchmark_group("compile");
        g.bench_with_input(BenchmarkId::from_parameter(&name), &json, |b, json| {
            b.iter(|| tson::compile_json(black_box(json)).unwrap());
        });
        g.finish();

        let mut g = c.benchmark_group("encode");
        g.bench_with_input(BenchmarkId::from_parameter(&name), &doc, |b, doc| {
            b.iter(|| tson::to_bytes(black_box(doc)).unwrap());
        });
        g.finish();

        let mut g = c.benchmark_group("decode");
        g.bench_with_input(BenchmarkId::from_parameter(&name), &bytes, |b, bytes| {
            b.iter(|| tson::from_bytes(black_box(bytes)).unwrap());
        });
        g.finish();

        let mut g = c.benchmark_group("decompile");
        g.bench_with_input(BenchmarkId::from_parameter(&name), &doc, |b, doc| {
            b.iter(|| tson::decompile_to_value(black_box(doc)).unwrap());
        });
        g.finish();

        let mut g = c.benchmark_group("roundtrip");
        g.bench_with_input(BenchmarkId::from_parameter(&name), &json, |b, json| {
            b.iter(|| {
                let doc = tson::compile_json(black_box(json)).unwrap();
                let bytes = tson::to_bytes(&doc).unwrap();
                let back = tson::from_bytes(&bytes).unwrap();
                tson::decompile_to_value(&back).unwrap()
            });
        });
        g.finish();
    }
}

criterion_group!(benches, bench_pipeline);
criterion_main!(benches);
