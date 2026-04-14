//! Microbenchmarks comparing the production `serde_json` request parser
//! against the experimental `simd-json` fast path.

use criterion::{criterion_group, criterion_main, black_box, Criterion};

use benchmarks::sample_request_bytes;

fn bench_request_parse(c: &mut Criterion) {
    let sample = sample_request_bytes();
    let mut group = c.benchmark_group("request_parse");

    group.bench_function("serde_json", |b| {
        b.iter(|| {
            let v: serde_json::Value =
                serde_json::from_slice(black_box(&sample)).expect("sample parses");
            black_box(v);
        })
    });

    group.bench_function("simd_json", |b| {
        b.iter(|| {
            let mut buf = sample.clone();
            let parsed = experimental_simd_json::FastJsonParser::parse_request_owned(
                black_box(&mut buf),
            )
            .expect("sample parses");
            black_box(parsed);
        })
    });

    group.finish();
}

criterion_group!(benches, bench_request_parse);
criterion_main!(benches);
