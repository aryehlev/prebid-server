//! Microbenchmark for the analytics fan-out runner.
//!
//! Dispatches 1000 auction events through an [`AnalyticsRunner`] wrapping
//! three [`NullAnalyticsBackend`] modules, driven by a multi-thread tokio
//! runtime created fresh per benchmark iteration.

use std::sync::Arc;

use criterion::{black_box, criterion_group, criterion_main, Criterion};

use analytics::{
    AnalyticsRunner, AuctionObject, NullAnalyticsBackend, PbsAnalyticsModule,
};
use tokio::runtime::Runtime;

const EVENT_COUNT: usize = 1000;

fn bench_analytics_fanout(c: &mut Criterion) {
    let mut group = c.benchmark_group("analytics_fanout");

    group.bench_function("three_null_backends_1000_events", |b| {
        b.iter(|| {
            let rt = Runtime::new().expect("tokio runtime");
            let runner = AnalyticsRunner::with_modules(vec![
                Arc::new(NullAnalyticsBackend::new())
                    as Arc<dyn PbsAnalyticsModule + Send + Sync>,
                Arc::new(NullAnalyticsBackend::new())
                    as Arc<dyn PbsAnalyticsModule + Send + Sync>,
                Arc::new(NullAnalyticsBackend::new())
                    as Arc<dyn PbsAnalyticsModule + Send + Sync>,
            ]);

            rt.block_on(async {
                for i in 0..EVENT_COUNT {
                    let evt = AuctionObject::new(
                        format!("req-{i}"),
                        "acct-bench",
                        200,
                        serde_json::json!({"i": i}),
                    );
                    runner.log_auction_object(black_box(&evt)).await;
                }
            });
        })
    });

    group.finish();
}

criterion_group!(benches, bench_analytics_fanout);
criterion_main!(benches);
