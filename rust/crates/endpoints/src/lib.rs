use axum::{
    extract::{Json, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};

/// Shared application state threaded through axum handlers.
pub type AppState = Arc<AppStateInner>;

pub struct AppStateInner {
    pub exchange: pbs_exchange::Exchange,
    pub version: String,
    pub revision: String,
    /// Static bidder info map: bidder name -> info JSON
    pub bidder_info: HashMap<String, serde_json::Value>,
    /// Bidder params JSON schemas: bidder name -> schema JSON
    pub bidder_params: HashMap<String, serde_json::Value>,
    /// Host cookie config
    pub host_cookie: HostCookieConfig,
    /// Status response override
    pub status_response: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct HostCookieConfig {
    pub cookie_name: String,
    pub family: String,
    pub domain: String,
    pub opt_out_url: String,
    pub opt_in_url: String,
    pub ttl_days: i64,
}

// ──────────────────────────────────────────────────────────────────────────────
// GET /status
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct StatusResponse {
    pub ok: &'static str,
}

pub async fn status_handler(State(state): State<AppState>) -> Response {
    if let Some(ref custom) = state.status_response {
        return (StatusCode::OK, custom.clone()).into_response();
    }
    (StatusCode::OK, Json(StatusResponse { ok: "ok" })).into_response()
}

// ──────────────────────────────────────────────────────────────────────────────
// GET /version
// ──────────────────────────────────────────────────────────────────────────────

pub async fn version_handler(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "revision": state.revision,
        "version":  state.version,
    }))
}

// ──────────────────────────────────────────────────────────────────────────────
// POST /openrtb2/auction
// ──────────────────────────────────────────────────────────────────────────────

pub async fn auction_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(bid_request): Json<openrtb::BidRequest>,
) -> Response {
    if bid_request.id.is_empty() {
        return (StatusCode::BAD_REQUEST, "request.id is required").into_response();
    }
    if bid_request.imp.is_empty() {
        return (StatusCode::BAD_REQUEST, "request.imp must contain at least one impression").into_response();
    }

    let auction_req = pbs_exchange::AuctionRequest {
        bid_request,
        account: None,
        user_syncs: None,
        start_time: std::time::Instant::now(),
    };

    match state.exchange.hold_auction(auction_req).await {
        Ok(auction_response) => {
            (StatusCode::OK, Json(auction_response.bid_response)).into_response()
        }
        Err(e) => {
            tracing::error!("Auction error: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// POST /openrtb2/video
// ──────────────────────────────────────────────────────────────────────────────

pub async fn video_auction_handler(
    State(state): State<AppState>,
    Json(bid_request): Json<openrtb::BidRequest>,
) -> Response {
    // Video endpoint reuses the same auction engine; video-specific
    // request building (pod splitting etc.) is a future enhancement.
    let auction_req = pbs_exchange::AuctionRequest {
        bid_request,
        account: None,
        user_syncs: None,
        start_time: std::time::Instant::now(),
    };

    match state.exchange.hold_auction(auction_req).await {
        Ok(r) => (StatusCode::OK, Json(r.bid_response)).into_response(),
        Err(e) => {
            tracing::error!("Video auction error: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// GET /openrtb2/amp
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct AmpParams {
    /// The stored request tag ID
    pub tag_id: Option<String>,
    pub w: Option<i32>,
    pub h: Option<i32>,
    pub ms: Option<String>,
    pub curl: Option<String>,
    pub slot: Option<String>,
    pub targeting: Option<String>,
    pub account: Option<String>,
    pub gdpr_applies: Option<bool>,
    pub gdpr_consent: Option<String>,
    pub us_privacy: Option<String>,
    pub consent_type: Option<i32>,
}

pub async fn amp_handler(
    State(_state): State<AppState>,
    Query(params): Query<AmpParams>,
) -> Response {
    let tag_id = match &params.tag_id {
        Some(t) if !t.is_empty() => t.clone(),
        _ => {
            return (StatusCode::BAD_REQUEST, "AMP request missing required tag_id query parameter").into_response();
        }
    };

    // In a full implementation this would load the stored request by tag_id,
    // merge AMP targeting parameters, run the auction, and return AMP targeting.
    // For now return a stub response indicating the endpoint is wired up.
    let response = serde_json::json!({
        "tag_id": tag_id,
        "targeting": {},
        "errors": { "prebid": [{"code": 999, "message": "stored request loading not yet implemented"}] }
    });
    (StatusCode::OK, Json(response)).into_response()
}

// ──────────────────────────────────────────────────────────────────────────────
// GET /info/bidders
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct BiddersQuery {
    pub enabled_only: Option<bool>,
}

pub async fn info_bidders_handler(
    State(state): State<AppState>,
    Query(q): Query<BiddersQuery>,
) -> Json<serde_json::Value> {
    let mut names: Vec<&str> = state.bidder_info.keys().map(|s| s.as_str()).collect();
    names.sort_unstable();

    // When enabled_only=true filter to bidders that have enabled:true in their info
    let names: Vec<&str> = if q.enabled_only.unwrap_or(false) {
        names
            .into_iter()
            .filter(|name| {
                state
                    .bidder_info
                    .get(*name)
                    .and_then(|v| v.get("enabled"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true)
            })
            .collect()
    } else {
        names
    };

    Json(serde_json::json!(names))
}

// ──────────────────────────────────────────────────────────────────────────────
// GET /info/bidders/:bidderName
// ──────────────────────────────────────────────────────────────────────────────

pub async fn info_bidder_detail_handler(
    State(state): State<AppState>,
    Path(bidder_name): Path<String>,
) -> Response {
    match state.bidder_info.get(&bidder_name) {
        Some(info) => (StatusCode::OK, Json(info.clone())).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            format!("bidder {} not found", bidder_name),
        )
            .into_response(),
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// GET /bidders/params
// ──────────────────────────────────────────────────────────────────────────────

pub async fn bidder_params_handler(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "$schema": "http://json-schema.org/draft-04/schema#",
        "title": "Bidder Params",
        "description": "Per-bidder OpenRTB extension parameter schemas",
        "oneOf": state.bidder_params.values().cloned().collect::<Vec<_>>()
    }))
}

// ──────────────────────────────────────────────────────────────────────────────
// GET /setuid
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct SetUidParams {
    pub bidder: Option<String>,
    pub uid: Option<String>,
    pub gdpr: Option<i32>,
    pub gdpr_consent: Option<String>,
    pub us_privacy: Option<String>,
    pub account: Option<String>,
    pub f: Option<String>, // "b" for iframe, "i" for pixel
}

pub async fn set_uid_handler(
    State(_state): State<AppState>,
    Query(params): Query<SetUidParams>,
) -> Response {
    let bidder = match &params.bidder {
        Some(b) if !b.is_empty() => b.clone(),
        _ => {
            return (StatusCode::BAD_REQUEST, "'bidder' query param is required").into_response();
        }
    };

    // In a full implementation this sets a cookie for the bidder UID and
    // performs GDPR consent checking. For now return 200 with the pixel/iframe.
    tracing::debug!("setuid: bidder={} uid={:?}", bidder, params.uid);

    let format = params.f.as_deref().unwrap_or("b");
    if format == "i" {
        // Return a 1x1 tracking pixel
        let pixel = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01\x00\x00\x00\x01\x08\x02\x00\x00\x00\x90wS\xde\x00\x00\x00\x0cIDATx\x9cc\xf8\x0f\x00\x00\x01\x01\x00\x05\x18\xd8N\x00\x00\x00\x00IEND\xaeB`\x82";
        return (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "image/png")],
            pixel.to_vec(),
        )
            .into_response();
    }

    StatusCode::OK.into_response()
}

// ──────────────────────────────────────────────────────────────────────────────
// GET /getuids
// ──────────────────────────────────────────────────────────────────────────────

pub async fn get_uids_handler(
    State(_state): State<AppState>,
    headers: HeaderMap,
) -> Json<serde_json::Value> {
    // In a full implementation this reads the host cookie and returns all
    // synced bidder UIDs. Return empty map for now.
    let _ = headers.get("Cookie");
    Json(serde_json::json!({ "buyeruids": {} }))
}

// ──────────────────────────────────────────────────────────────────────────────
// POST /cookie_sync
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct CookieSyncRequest {
    pub bidders: Option<Vec<String>>,
    pub gdpr: Option<i32>,
    pub gdpr_consent: Option<String>,
    pub us_privacy: Option<String>,
    pub limit: Option<i32>,
    pub coopSync: Option<bool>,
    pub filterSettings: Option<serde_json::Value>,
    pub account: Option<String>,
}

#[derive(Serialize)]
pub struct CookieSyncResponse {
    pub status: String,
    pub bidder_status: Vec<serde_json::Value>,
}

pub async fn cookie_sync_handler(
    State(_state): State<AppState>,
    Json(body): Json<CookieSyncRequest>,
) -> Json<CookieSyncResponse> {
    // Full implementation would check which bidders need syncing and
    // return iframe/redirect URLs. Return empty sync list for now.
    let _ = body;
    Json(CookieSyncResponse {
        status: "no_cookie".to_string(),
        bidder_status: Vec::new(),
    })
}

// ──────────────────────────────────────────────────────────────────────────────
// GET /event
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct EventParams {
    #[serde(rename = "t")]
    pub event_type: Option<String>, // "win", "imp"
    #[serde(rename = "b")]
    pub bid_id: Option<String>,
    #[serde(rename = "a")]
    pub account_id: Option<String>,
    #[serde(rename = "bidder")]
    pub bidder: Option<String>,
    #[serde(rename = "f")]
    pub format: Option<String>, // "b" blank, "i" image
    #[serde(rename = "ts")]
    pub timestamp: Option<i64>,
    #[serde(rename = "analytics")]
    pub analytics: Option<String>,
}

pub async fn event_handler(
    State(_state): State<AppState>,
    Query(params): Query<EventParams>,
) -> Response {
    tracing::debug!(
        "event: type={:?} bid={:?} account={:?}",
        params.event_type,
        params.bid_id,
        params.account_id
    );

    // Return 1x1 PNG pixel for image format, blank otherwise
    if params.format.as_deref() == Some("i") {
        let pixel = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01\x00\x00\x00\x01\x08\x02\x00\x00\x00\x90wS\xde\x00\x00\x00\x0cIDATx\x9cc\xf8\x0f\x00\x00\x01\x01\x00\x05\x18\xd8N\x00\x00\x00\x00IEND\xaeB`\x82";
        return (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "image/png")],
            pixel.to_vec(),
        )
            .into_response();
    }

    StatusCode::OK.into_response()
}

// ──────────────────────────────────────────────────────────────────────────────
// POST /vtrack
// ──────────────────────────────────────────────────────────────────────────────

pub async fn vtrack_handler(
    State(_state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
    body: axum::body::Bytes,
) -> Response {
    let account = params.get("a").cloned().unwrap_or_default();
    tracing::debug!("vtrack: account={} body_len={}", account, body.len());
    // Full implementation rewrites VAST XML with tracking URLs.
    // Return the body unchanged for now.
    (StatusCode::OK, body).into_response()
}

// ──────────────────────────────────────────────────────────────────────────────
// GET / — index
// ──────────────────────────────────────────────────────────────────────────────

pub async fn index_handler() -> &'static str {
    "prebid-server (Rust port)"
}

// ──────────────────────────────────────────────────────────────────────────────
// Router
// ──────────────────────────────────────────────────────────────────────────────

pub fn create_router(state: AppState) -> axum::Router {
    axum::Router::new()
        // Core
        .route("/", axum::routing::get(index_handler))
        .route("/status", axum::routing::get(status_handler))
        .route("/version", axum::routing::get(version_handler))
        // Auction
        .route("/openrtb2/auction", axum::routing::post(auction_handler))
        .route("/openrtb2/video", axum::routing::post(video_auction_handler))
        .route("/openrtb2/amp", axum::routing::get(amp_handler))
        // Bidder info
        .route("/info/bidders", axum::routing::get(info_bidders_handler))
        .route("/info/bidders/:bidderName", axum::routing::get(info_bidder_detail_handler))
        .route("/bidders/params", axum::routing::get(bidder_params_handler))
        // User sync
        .route("/cookie_sync", axum::routing::post(cookie_sync_handler))
        .route("/setuid", axum::routing::get(set_uid_handler))
        .route("/getuids", axum::routing::get(get_uids_handler))
        // Events
        .route("/event", axum::routing::get(event_handler))
        .route("/vtrack", axum::routing::post(vtrack_handler))
        .with_state(state)
}
