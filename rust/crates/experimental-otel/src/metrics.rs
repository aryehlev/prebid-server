//! Prebid Server metric instruments backed by the OpenTelemetry global meter.
//!
//! [`PbsMeters`] is a lazily-constructable bundle of instruments with stable
//! names that can be incremented/recorded from anywhere in the server.
//! Construction uses `opentelemetry::global::meter`, so the bundle will
//! attach to whichever meter provider is installed at the time of the call
//! (including the default no-op meter provider, which is a valid target
//! for tests).

use opentelemetry::metrics::{Counter, Histogram, Meter, UpDownCounter};
use opentelemetry::KeyValue;

/// Scope / instrumentation name used for the meter.
pub const METER_SCOPE: &str = "experimental-otel/pbs";

/// Bundle of PBS-wide metric instruments.
///
/// Construct once at startup with [`PbsMeters::new`] and clone cheaply
/// (all fields are `Clone`).
#[derive(Clone)]
pub struct PbsMeters {
    /// `pbs.requests.total` — total PBS requests. Labels: `endpoint`, `status`.
    pub requests_total: Counter<u64>,
    /// `pbs.request.duration` — request duration in milliseconds.
    pub request_duration: Histogram<f64>,
    /// `pbs.adapter.requests` — adapter call counts. Labels: `bidder`, `status`.
    pub adapter_requests: Counter<u64>,
    /// `pbs.adapter.bids` — bid counts from adapters. Labels: `bidder`, `has_bid`.
    pub adapter_bids: Counter<u64>,
    /// `pbs.cache.operations` — cache op counts. Labels: `operation`, `status`.
    pub cache_operations: Counter<u64>,
    /// `pbs.active_connections` — current in-flight connection count.
    pub active_connections: UpDownCounter<i64>,
}

impl PbsMeters {
    /// Build a new bundle using `opentelemetry::global::meter`.
    ///
    /// This can be called multiple times; each call produces independent
    /// instrument handles referring to the same underlying metric stream.
    pub fn new() -> Self {
        let meter: Meter = opentelemetry::global::meter(METER_SCOPE);
        Self::with_meter(&meter)
    }

    /// Build a bundle with a caller-supplied meter (useful for tests or
    /// advanced configurations that want a named meter).
    pub fn with_meter(meter: &Meter) -> Self {
        let requests_total = meter
            .u64_counter("pbs.requests.total")
            .with_description("Total PBS requests processed, partitioned by endpoint and status.")
            .with_unit("1")
            .build();

        let request_duration = meter
            .f64_histogram("pbs.request.duration")
            .with_description("PBS request handler duration.")
            .with_unit("ms")
            .build();

        let adapter_requests = meter
            .u64_counter("pbs.adapter.requests")
            .with_description("Adapter HTTP calls, partitioned by bidder and status.")
            .with_unit("1")
            .build();

        let adapter_bids = meter
            .u64_counter("pbs.adapter.bids")
            .with_description("Bids returned by adapters, partitioned by bidder and presence.")
            .with_unit("1")
            .build();

        let cache_operations = meter
            .u64_counter("pbs.cache.operations")
            .with_description("Prebid Cache operations, partitioned by operation and status.")
            .with_unit("1")
            .build();

        let active_connections = meter
            .i64_up_down_counter("pbs.active_connections")
            .with_description("Currently-active server connections.")
            .with_unit("1")
            .build();

        Self {
            requests_total,
            request_duration,
            adapter_requests,
            adapter_bids,
            cache_operations,
            active_connections,
        }
    }

    /// Convenience: record a completed request.
    pub fn observe_request(&self, endpoint: &str, status: &str, duration_ms: f64) {
        let attrs = [
            KeyValue::new("endpoint", endpoint.to_string()),
            KeyValue::new("status", status.to_string()),
        ];
        self.requests_total.add(1, &attrs);
        self.request_duration.record(duration_ms, &attrs);
    }

    /// Convenience: record an adapter call outcome.
    pub fn observe_adapter_call(&self, bidder: &str, status: &str, had_bid: bool) {
        self.adapter_requests.add(
            1,
            &[
                KeyValue::new("bidder", bidder.to_string()),
                KeyValue::new("status", status.to_string()),
            ],
        );
        self.adapter_bids.add(
            1,
            &[
                KeyValue::new("bidder", bidder.to_string()),
                KeyValue::new("has_bid", had_bid.to_string()),
            ],
        );
    }

    /// Convenience: record a cache op outcome.
    pub fn observe_cache(&self, operation: &str, status: &str) {
        self.cache_operations.add(
            1,
            &[
                KeyValue::new("operation", operation.to_string()),
                KeyValue::new("status", status.to_string()),
            ],
        );
    }
}

impl Default for PbsMeters {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_create_meters_and_increment() {
        // Default global meter provider is a no-op; all calls should be safe.
        let m = PbsMeters::new();
        m.observe_request("/openrtb2/auction", "200", 12.5);
        m.observe_adapter_call("appnexus", "ok", true);
        m.observe_adapter_call("rubicon", "timeout", false);
        m.observe_cache("put", "ok");
        m.active_connections.add(1, &[]);
        m.active_connections.add(-1, &[]);
    }

    #[test]
    fn can_clone_bundle() {
        let m = PbsMeters::new();
        let _c = m.clone();
    }
}
