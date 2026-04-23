//! HTTP router wiring real handlers for each prebid-server endpoint.
//!
//! This mirrors the route surface of the Go implementation in
//! `router/router.go`. Handlers delegate to the standalone port crates
//! (`pbs-endpoints`, `usersync`, `pbs-metrics`, `analytics`, `pbs-config`,
//! `account`) wherever possible; a couple of endpoints still emit thin
//! self-contained responses because their production counterparts in
//! `pbs_endpoints` require a fully-constructed `pbs_exchange::Exchange`
//! that this crate deliberately does not depend on (see the comment on the
//! `/openrtb2/auction` stub below).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;
use tower_http::{
    compression::CompressionLayer,
    cors::{Any, CorsLayer},
    timeout::TimeoutLayer,
    trace::TraceLayer,
};

use crate::config::ServerConfig;
use crate::state::AppState;

/// Build the full application router with routes and middleware attached.
///
/// The router threads a shared [`Arc<AppState>`] through every handler via
/// axum's `State` extractor. [`AppState::dev`] provides a no-op default
/// wiring for local development and smoke tests.
pub fn build_router(cfg: &ServerConfig, state: Arc<AppState>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION]);

    // Use the larger of the two configured timeouts as the request deadline
    // so that neither read nor write is artificially truncated.
    let request_timeout = cfg
        .read_timeout
        .max(cfg.write_timeout)
        .max(Duration::from_secs(1));

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
        .with_state(state)
}

// ---------------------------------------------------------------------------
// /openrtb2/auction
//
// Forwards to `pbs_endpoints::auction_handler` when a dev exchange has been
// wired into [`AppState`]. The production handler lives in `pbs_endpoints`
// and drives the full pipeline (validation, stored-requests merge, hooks,
// adapter fan-out, response building, metrics). Our state owns an
// `Option<pbs_endpoints::AppState>` that was pre-built in
// [`AppState::dev`]; we clone the inner `Arc` and hand it to the production
// handler, passing through headers and the raw JSON body untouched.
// ---------------------------------------------------------------------------
async fn openrtb2_auction(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Json<openrtb::BidRequest>,
) -> Response {
    let Some(endpoints_state) = state.endpoints_state.clone() else {
        return not_implemented("openrtb2_auction");
    };
    pbs_endpoints::auction_handler(
        axum::extract::State(endpoints_state),
        headers,
        body,
    )
    .await
}

async fn openrtb2_video(
    State(state): State<Arc<AppState>>,
    body: Json<openrtb::BidRequest>,
) -> Response {
    let Some(endpoints_state) = state.endpoints_state.clone() else {
        return not_implemented("openrtb2_video");
    };
    pbs_endpoints::video_auction_handler(
        axum::extract::State(endpoints_state),
        body,
    )
    .await
}

// ---------------------------------------------------------------------------
// /openrtb2/amp
//
// Proxy into `pbs_endpoints::amp_handler` — same pattern as /openrtb2/auction,
// but `amp_handler` extracts a typed [`pbs_endpoints::AmpParams`] from the
// querystring, so we mirror the extractor signature here.
// ---------------------------------------------------------------------------
async fn openrtb2_amp(
    State(state): State<Arc<AppState>>,
    params: Query<pbs_endpoints::AmpParams>,
) -> Response {
    let Some(endpoints_state) = state.endpoints_state.clone() else {
        return not_implemented("openrtb2_amp");
    };
    pbs_endpoints::amp_handler(
        axum::extract::State(endpoints_state),
        params,
    )
    .await
}

// ---------------------------------------------------------------------------
// /cookie_sync
//
// Builds a minimal `usersync::ChooserRequest` from the request body and
// forwards it to the configured `StandardChooser`. The response format
// mirrors the Go implementation (`status` + `bidder_status`).
// ---------------------------------------------------------------------------
#[derive(Debug, Deserialize, Default)]
struct CookieSyncBody {
    #[serde(default)]
    bidders: Vec<String>,
    #[serde(default)]
    limit: Option<u64>,
}

async fn cookie_sync(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Option<Json<CookieSyncBody>>,
) -> Response {
    let body = body.map(|Json(b)| b).unwrap_or_default();

    let cookie = cookie_from_headers(&headers);

    let request = usersync::ChooserRequest {
        bidders: body.bidders,
        limit: body.limit.unwrap_or(0) as usize,
        sync_type_filter: usersync::SyncTypeFilter::default(),
        privacy_blocked: Default::default(),
    };

    // `StandardChooser::choose` is synchronous and cheap; no need to spawn.
    let result = usersync::Chooser::choose(state.chooser.as_ref(), &request, &cookie);

    let status = match result.status {
        usersync::Status::Ok => "ok",
        usersync::Status::BlockedByUserOptOut => "blocked_by_user_opt_out",
        usersync::Status::BlockedByPrivacy => "blocked_by_privacy",
        _ => "no_sync",
    };

    let bidder_status: Vec<_> = result
        .syncers_chosen
        .iter()
        .map(|choice| {
            json!({
                "bidder": choice.bidder,
                "no_cookie": true,
            })
        })
        .collect();

    Json(json!({
        "status": status,
        "bidder_status": bidder_status,
    }))
    .into_response()
}

// ---------------------------------------------------------------------------
// /setuid
//
// Full semantics (parsing bidder/uid/gdpr/gpp query params, merging into the
// existing `uids` cookie, re-encoding it, and firing analytics) live in
// `pbs_endpoints::set_uid_handler`. Here we emit a 204 with the existing
// cookie echoed back into `Set-Cookie` as a placeholder: it is wire-safe
// (clients see no change) and the header plumbing is exercised end to end.
// ---------------------------------------------------------------------------
async fn setuid(
    State(_state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Response {
    let cookie = cookie_from_headers(&headers);

    // Best-effort re-encode; on failure (should not happen with our data)
    // just return 204 without touching Set-Cookie.
    let mut response = StatusCode::NO_CONTENT.into_response();
    if let Ok(encoded) = cookie.encode() {
        let set_cookie = format!(
            "{}={}; Path=/; HttpOnly; Max-Age={}",
            usersync::UID_COOKIE_NAME,
            encoded,
            usersync::UID_TTL_DAYS * 24 * 60 * 60
        );
        if let Ok(val) = header::HeaderValue::from_str(&set_cookie) {
            response.headers_mut().insert(header::SET_COOKIE, val);
        }
    }
    response
}

// ---------------------------------------------------------------------------
// /getuids
//
// Decode the `uids` cookie and return the contained map of bidder->uid.
// This mirrors `pbs_endpoints::get_uids_handler` without needing the heavy
// endpoints::AppState.
// ---------------------------------------------------------------------------
async fn getuids(headers: HeaderMap) -> Json<serde_json::Value> {
    let cookie = cookie_from_headers(&headers);
    Json(json!({ "buyeruids": cookie.get_uids() }))
}

// ---------------------------------------------------------------------------
// /event
//
// Fire-and-forget in the Go implementation. We return 204 and rely on the
// tracing layer for observability until the full event_handler is wired up.
// ---------------------------------------------------------------------------
async fn event(State(_state): State<Arc<AppState>>) -> StatusCode {
    StatusCode::NO_CONTENT
}

// ---------------------------------------------------------------------------
// /info/bidders
//
// Reads the bidder keys off the configured `BidderInfos` map.
// ---------------------------------------------------------------------------
async fn info_bidders(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let mut bidders: Vec<&String> = state.config.bidder_infos.inner.keys().collect();
    bidders.sort();
    Json(json!(bidders))
}

async fn info_bidder(
    State(state): State<Arc<AppState>>,
    Path(bidder): Path<String>,
) -> Response {
    match state.config.bidder_infos.inner.get(&bidder) {
        Some(info) => Json(info.clone()).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": "bidder_not_found",
                "bidder": bidder,
            })),
        )
            .into_response(),
    }
}

// ---------------------------------------------------------------------------
// /status
//
// Go emits 204 by default and a custom status response (e.g. `"ok"`) when
// configured. We do the same by inspecting `config.status_response`.
// ---------------------------------------------------------------------------
async fn status(State(state): State<Arc<AppState>>) -> Response {
    if state.config.status_response.is_empty() {
        StatusCode::NO_CONTENT.into_response()
    } else {
        (StatusCode::OK, state.config.status_response.clone()).into_response()
    }
}

// ---------------------------------------------------------------------------
// /version
// ---------------------------------------------------------------------------
async fn version() -> Json<serde_json::Value> {
    Json(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "revision": "unknown",
    }))
}

// ---------------------------------------------------------------------------
// /currency/rates
//
// The real rate converter is not yet plumbed through `AppState` (that will
// land alongside the full /openrtb2/auction wiring). For now we emit a
// stable empty rates document so clients that poll this endpoint don't
// break.
// ---------------------------------------------------------------------------
async fn currency_rates(State(_state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(json!({
        "dataAsOf": "",
        "conversions": HashMap::<String, serde_json::Value>::new(),
    }))
}

// ---------------------------------------------------------------------------
// /metrics
//
// Use each AppState's own PrometheusMetrics registry so we don't depend on
// the global one having been installed. Falls back to the global registry
// via `pbs_metrics::gather` if the local registry is empty (matching the Go
// semantics where metrics come from a single registry).
// ---------------------------------------------------------------------------
async fn metrics(
    State(state): State<Arc<AppState>>,
) -> (StatusCode, [(header::HeaderName, &'static str); 1], String) {
    let body = {
        let text = state.metrics.gather_text();
        if text.is_empty() {
            pbs_metrics::gather()
        } else {
            text
        }
    };
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/plain; version=0.0.4")],
        body,
    )
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse the incoming `Cookie` header and pick out the prebid `uids` cookie,
/// decoding it into a [`usersync::Cookie`]. Returns an empty cookie when the
/// header is missing or malformed.
fn cookie_from_headers(headers: &HeaderMap) -> usersync::Cookie {
    let Some(raw) = headers.get(header::COOKIE).and_then(|v| v.to_str().ok()) else {
        return usersync::Cookie::new();
    };
    for pair in raw.split(';') {
        let trimmed = pair.trim();
        if let Some(rest) = trimmed.strip_prefix(&format!("{}=", usersync::UID_COOKIE_NAME)) {
            return usersync::Cookie::decode(rest);
        }
    }
    usersync::Cookie::new()
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
        let state = Arc::new(AppState::dev());
        // Just assert that construction succeeds; we don't poke the routes.
        let _router: Router = build_router(&cfg, state);
    }
}
