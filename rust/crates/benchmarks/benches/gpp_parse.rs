//! Microbenchmark for the GPP top-level header parser.

use criterion::{black_box, criterion_group, criterion_main, Criterion};

use pbs_privacy::gpp::header::parse_gpp_header;

/// A canonical-looking GPP string: a base64url-encoded header segment
/// followed by a US Privacy ("usp") payload.
const SAMPLE_GPP: &str = "DBABMA~1YNN";

fn bench_gpp_parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("gpp_parse");

    group.bench_function("parse_gpp_header", |b| {
        b.iter(|| {
            let result = parse_gpp_header(black_box(SAMPLE_GPP));
            black_box(result.ok());
        })
    });

    group.finish();
}

criterion_group!(benches, bench_gpp_parse);
criterion_main!(benches);
