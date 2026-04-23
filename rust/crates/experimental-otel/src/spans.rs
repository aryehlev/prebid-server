//! Semantic-convention helpers for creating `tracing` spans that mirror
//! the well-known Prebid Server operation shapes: auctions, adapter calls,
//! cache operations, and stored-request fetches.
//!
//! These helpers are thin wrappers around `tracing::info_span!` with field
//! names chosen to match (or extend) common OpenTelemetry semantic
//! conventions. They are safe to call regardless of whether an OTLP
//! exporter is installed.

use tracing::{info_span, Span};

/// Span covering a full auction request handler.
///
/// Fields:
/// - `otel.name` — stable span name (`pbs.auction`)
/// - `pbs.account_id`
/// - `pbs.request_id`
pub fn auction_span(account_id: &str, request_id: &str) -> Span {
    info_span!(
        "pbs.auction",
        otel.name = "pbs.auction",
        pbs.account_id = %account_id,
        pbs.request_id = %request_id,
    )
}

/// Span covering a single adapter/bidder HTTP call.
///
/// Fields:
/// - `otel.name` — `pbs.adapter`
/// - `pbs.bidder`
pub fn adapter_span(bidder_name: &str) -> Span {
    info_span!(
        "pbs.adapter",
        otel.name = "pbs.adapter",
        pbs.bidder = %bidder_name,
    )
}

/// Span covering a Prebid Cache operation (put/get/delete).
///
/// Fields:
/// - `otel.name` — `pbs.cache`
/// - `pbs.cache.operation`
pub fn cache_span(operation: &str) -> Span {
    info_span!(
        "pbs.cache",
        otel.name = "pbs.cache",
        pbs.cache.operation = %operation,
    )
}

/// Span covering a stored-request (config, imps, responses) fetch.
///
/// Fields:
/// - `otel.name` — `pbs.stored_request`
/// - `pbs.stored_request.source`
pub fn stored_request_span(source: &str) -> Span {
    info_span!(
        "pbs.stored_request",
        otel.name = "pbs.stored_request",
        pbs.stored_request.source = %source,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_constructors_do_not_panic() {
        let _a = auction_span("acct-1", "req-abc");
        let _b = adapter_span("appnexus");
        let _c = cache_span("put");
        let _d = stored_request_span("postgres");
    }

    #[test]
    fn spans_can_be_entered() {
        let s = auction_span("acct-1", "req-abc");
        let _e = s.enter();
    }
}
