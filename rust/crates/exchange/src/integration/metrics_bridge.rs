//! Metrics bridge.
//!
//! Thin convenience wrappers that let exchange call sites record auction /
//! adapter / cache metrics through a `dyn pbs_metrics::MetricsEngine`
//! without having to construct the full label structs inline.
//!
//! All helpers map HTTP-like status codes onto `pbs_metrics::RequestStatus`
//! values and degrade gracefully: if the underlying `MetricsEngine`
//! implementation only provides the default no-op bodies for the trait
//! methods we call, these helpers are themselves harmless no-ops.

use std::time::Duration;

use pbs_metrics::{
    AdapterBidStatus, AdapterLabels, MetricsEngine, RequestLabels, RequestStatus, RequestType,
};

/// Map an HTTP status code onto the closest `RequestStatus` variant we track.
fn status_from_http(status: u16) -> RequestStatus {
    match status {
        200..=299 => RequestStatus::Ok,
        400 | 422 => RequestStatus::BadInput,
        408 | 504 => RequestStatus::Timeout,
        _ => RequestStatus::Err,
    }
}

/// Map an endpoint string onto a `RequestType`. Unknown endpoints default to
/// `OpenRTB2Web` so a misspelled endpoint does not blow up metric emission.
fn request_type_from_endpoint(endpoint: &str) -> RequestType {
    match endpoint {
        "amp" | "/openrtb2/amp" => RequestType::Amp,
        "video" | "/openrtb2/video" => RequestType::Video,
        "app" | "openrtb2-app" => RequestType::OpenRTB2App,
        "dooh" | "openrtb2-dooh" => RequestType::OpenRTB2DOOH,
        _ => RequestType::OpenRTB2Web,
    }
}

/// Record a completed auction request through the metrics engine.
pub fn record_auction_request(m: &dyn MetricsEngine, endpoint: &str, status: u16) {
    let labels = RequestLabels {
        request_type: request_type_from_endpoint(endpoint),
        request_status: status_from_http(status),
    };
    m.record_request(&labels);
    m.record_request_by_type(endpoint, status_from_http(status));
}

/// Record how long an auction took, in milliseconds.
pub fn record_auction_duration(m: &dyn MetricsEngine, endpoint: &str, duration_ms: u64) {
    let labels = RequestLabels {
        request_type: request_type_from_endpoint(endpoint),
        request_status: RequestStatus::Ok,
    };
    m.record_request_time(&labels, Duration::from_millis(duration_ms));
    m.record_auction_duration(endpoint, duration_ms);
}

/// Record that a bidder adapter was called, and whether it returned any bids.
pub fn record_adapter_call(m: &dyn MetricsEngine, bidder: &str, has_bid: bool) {
    let labels = AdapterLabels {
        adapter: bidder.to_string(),
        adapter_bid_type: if has_bid {
            AdapterBidStatus::Bid
        } else {
            AdapterBidStatus::NoBid
        },
        no_cookie: false,
    };
    m.record_adapter_request(&labels);
    m.record_bidder_request(bidder);
}

/// Record a cache (prebid cache) operation outcome.
pub fn record_cache_op(m: &dyn MetricsEngine, _op: &str, ok: bool) {
    m.record_prebid_cache_request_time(ok, Duration::from_millis(0));
}
