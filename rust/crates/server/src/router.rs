//! HTTP router skeleton.
//!
//! Mirrors the route surface of the Go implementation in
//! `router/router.go`.  Every handler is currently a placeholder that returns
//! a small JSON stub (or `204 No Content`); the real implementations will
//! land as the surrounding crates come online.

use std::time::Duration;

use axum::{
    extract::Path,
    http::{header, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde_json::json;
use tower_http::{
    compression::CompressionLayer,
    cors::{Any, CorsLayer},
    timeout::TimeoutLayer,
    trace::TraceLayer,
};

use crate::config::ServerConfig;

/// Build the full application router with routes and middleware attached.
///
/// The router is deliberately stateless: each handler is an inline async
/// function returning a placeholder response.  As real endpoints are wired
/// up they will graduate out of this file into their own modules (probably
/// living in the `endpoints` crate).
pub fn build_router(cfg: &ServerConfig) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION]);

    // Use the larger of the two configured timeouts as the request deadline
    // so that neither read nor write is artificially truncated.
    let request_timeout = cfg.read_timeout.max(cfg.write_timeout).max(Duration::from_secs(1));

    Router::new()
        // ---- OpenRTB auction surface ----------------------------------------
        .route("/openrtb2/auction", post(openrtb2_auction))
        .route("/openrtb2/video", post(openrtb2_video))
        .route("/openrtb2/amp", get(openrtb2_amp))
        // ---- Cookie-sync / UID management -----------------------------------
        .route("/cookie_sync", post(cookie_sync))
        .route("/setuid", get(setuid).post(setuid))
        .route("/getuids", get(getuids))
        // ---- Event + info endpoints -----------------------------------------
        .route("/event", get(event))
        .route("/info/bidders", get(info_bidders))
        .route("/info/bidders/:bidder", get(info_bidder))
        // ---- Operational endpoints ------------------------------------------
        .route("/status", get(status))
        .route("/version", get(version))
        .route("/currency/rates", get(currency_rates))
        .route("/metrics", get(metrics))
        // ---- Middleware stack -----------------------------------------------
        // Layered outside-in: tracing runs first so that every request is
        // logged, compression is applied on the way out, a hard timeout
        // bounds every handler, and CORS adds the usual headers.
        .layer(TraceLayer::new_for_http())
        .layer(CompressionLayer::new())
        .layer(TimeoutLayer::new(request_timeout))
        .layer(cors)
}

// ---------------------------------------------------------------------------
// Placeholder handlers.
//
// Each of these returns a minimal stub response so that integration tests and
// smoke tests have something to talk to.  They MUST be replaced with real
// implementations before the Rust port ships.
// ---------------------------------------------------------------------------

async fn openrtb2_auction() -> Response {
    not_implemented("openrtb2_auction")
}

async fn openrtb2_video() -> Response {
    not_implemented("openrtb2_video")
}

async fn openrtb2_amp() -> Response {
    not_implemented("openrtb2_amp")
}

async fn cookie_sync() -> Response {
    not_implemented("cookie_sync")
}

async fn setuid() -> StatusCode {
    // Go implementation returns an empty 200/204 for no-op sync requests.
    StatusCode::NO_CONTENT
}

async fn getuids() -> Json<serde_json::Value> {
    Json(json!({ "buyeruids": {} }))
}

async fn event() -> StatusCode {
    // Event notifications are fire-and-forget in the Go implementation.
    StatusCode::NO_CONTENT
}

async fn info_bidders() -> Json<serde_json::Value> {
    Json(json!([]))
}

async fn info_bidder(Path(bidder): Path<String>) -> Json<serde_json::Value> {
    Json(json!({
        "bidder": bidder,
        "status": "unknown",
        "message": "bidder info not yet implemented",
    }))
}

async fn status() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok" }))
}

async fn version() -> Json<serde_json::Value> {
    Json(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "revision": "unknown",
    }))
}

async fn currency_rates() -> Json<serde_json::Value> {
    Json(json!({
        "dataAsOf": "",
        "conversions": {},
    }))
}

async fn metrics() -> (StatusCode, [(header::HeaderName, &'static str); 1], &'static str) {
    // Placeholder until the prometheus exporter is wired in.
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/plain; version=0.0.4")],
        "# prebid-server metrics placeholder\n",
    )
}

fn not_implemented(name: &'static str) -> Response {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(json!({
            "error": "not_implemented",
            "endpoint": name,
        })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_router_with_defaults() {
        let cfg = ServerConfig::default();
        // Just assert that construction succeeds; we don't poke the routes.
        let _router: Router = build_router(&cfg);
    }
}
