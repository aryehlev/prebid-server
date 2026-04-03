use axum::{
    extract::{Json, Path, Query, State},
    http::{header::{COOKIE, SET_COOKIE}, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use base64::{engine::general_purpose::STANDARD, Engine};
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
// UserSyncCookie abstraction
// ──────────────────────────────────────────────────────────────────────────────

/// Represents the prebid UID cookie ("uids")
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UserSyncCookie {
    #[serde(default)]
    pub uids: HashMap<String, UidEntry>,
    #[serde(default)]
    pub optout: Option<bool>,
}

/// A single bidder UID with expiry timestamp (Unix seconds)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UidEntry {
    pub uid: String,
    /// Unix timestamp (seconds) after which this UID is considered stale
    pub expires: i64,
}

/// Default TTL for a UID: 14 days in seconds
const UID_TTL_SECS: i64 = 14 * 24 * 60 * 60;

/// Default cookie name used by prebid-server
const DEFAULT_COOKIE_NAME: &str = "uids";

impl UserSyncCookie {
    /// Parse a `UserSyncCookie` from the `Cookie` request header.
    /// The cookie value is expected to be a base64-encoded JSON string.
    pub fn from_request(headers: &HeaderMap, cookie_name: &str) -> Self {
        let name = if cookie_name.is_empty() { DEFAULT_COOKIE_NAME } else { cookie_name };
        if let Some(cookie_hdr) = headers.get(COOKIE) {
            if let Ok(cookie_str) = cookie_hdr.to_str() {
                for pair in cookie_str.split(';') {
                    let pair = pair.trim();
                    if let Some((k, v)) = pair.split_once('=') {
                        if k.trim() == name {
                            return Self::decode(v.trim());
                        }
                    }
                }
            }
        }
        Self::default()
    }

    /// Decode a base64-encoded cookie value into a `UserSyncCookie`.
    fn decode(value: &str) -> Self {
        let bytes = match STANDARD.decode(value) {
            Ok(b) => b,
            Err(_) => return Self::default(),
        };
        serde_json::from_slice::<Self>(&bytes).unwrap_or_default()
    }

    /// Encode this cookie to a base64 string suitable for a `Set-Cookie` value.
    pub fn encode(&self) -> String {
        let json = serde_json::to_vec(self).unwrap_or_default();
        STANDARD.encode(&json)
    }

    /// Return true if the bidder has a non-expired UID
    pub fn has_valid_uid(&self, bidder: &str) -> bool {
        let now = chrono::Utc::now().timestamp();
        self.uids.get(bidder).map(|e| e.expires > now).unwrap_or(false)
    }

    /// Set a UID for a bidder with a default TTL
    pub fn set_uid(&mut self, bidder: &str, uid: String) {
        let expires = chrono::Utc::now().timestamp() + UID_TTL_SECS;
        self.uids.insert(bidder.to_string(), UidEntry { uid, expires });
    }

    /// Build the `Set-Cookie` header value string
    pub fn build_set_cookie_header(&self, cookie_name: &str, ttl_days: i64, domain: &str) -> String {
        let name = if cookie_name.is_empty() { DEFAULT_COOKIE_NAME } else { cookie_name };
        let value = self.encode();
        let max_age_days = if ttl_days > 0 { ttl_days } else { 90 };
        let max_age_secs = max_age_days * 24 * 60 * 60;
        let mut hdr = format!(
            "{}={}; Max-Age={}; Path=/; HttpOnly; SameSite=None; Secure",
            name, value, max_age_secs
        );
        if !domain.is_empty() {
            hdr.push_str(&format!("; Domain={}", domain));
        }
        hdr
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Static bidder sync URL map (redirect URLs; macros left as-is for now)
// Keys are the canonical bidder names used in the prebid cookie.
// ──────────────────────────────────────────────────────────────────────────────

fn bidder_sync_url(bidder: &str) -> Option<(&'static str, &'static str)> {
    // Returns (sync_type, url_template)
    match bidder {
        "appnexus" | "adnxs" => Some(("redirect", "https://ib.adnxs.com/getuid?{{.RedirectURL}}")),
        "openx" => Some(("redirect", "https://rtb.openx.net/sync/prebid?r={{.RedirectURL}}")),
        "pubmatic" => Some(("redirect", "https://image8.pubmatic.com/AdServer/ImgSync?p=159706&pu={{.RedirectURL}}")),
        "ix" => Some(("redirect", "https://ssum.casalemedia.com/usermatchredir?s=194962&cb={{.RedirectURL}}")),
        "sovrn" => Some(("redirect", "https://ap.lijit.com/pixel?redir={{.RedirectURL}}")),
        "adform" => Some(("redirect", "https://c1.adform.net/cookie?redirect_url={{.RedirectURL}}")),
        "33across" => Some(("iframe", "https://ssc-cms.33across.com/ps/?m=xch&rt=html&ru={{.RedirectURL}}&id=zzz000000000002zzz")),
        "criteo" => Some(("redirect", "https://ssp-sync.criteo.com/user-sync/redirect?profile=230&redir={{.RedirectURL}}")),
        "yieldmo" => Some(("redirect", "https://ads.yieldmo.com/pbsync?redirectUri={{.RedirectURL}}")),
        "sharethrough" => Some(("redirect", "https://match.sharethrough.com/FGMrCMMc/v1?redirectUri={{.RedirectURL}}")),
        "smaato" => Some(("redirect", "https://s.ad.smaato.net/c/?adExInit=p&redir={{.RedirectURL}}")),
        "conversant" => Some(("redirect", "https://prebid-match.dotomi.com/match/bounce/current?version=1&networkId=72582&rurl={{.RedirectURL}}")),
        "smartadserver" => Some(("redirect", "https://ssbsync-global.smartadserver.com/api/sync?callerId=5&redirectUri={{.RedirectURL}}")),
        "yieldlab" => Some(("redirect", "https://ad.yieldlab.net/mr?t=2&pid=9140838&r={{.RedirectURL}}")),
        "triplelift" => Some(("iframe", "https://eb2.3lift.com/sync?redir={{.RedirectURL}}")),
        _ => None,
    }
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
    _headers: HeaderMap,
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
    pub f: Option<String>, // "b" for blank/iframe, "i" for pixel
}

pub async fn set_uid_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<SetUidParams>,
) -> Response {
    let bidder = match &params.bidder {
        Some(b) if !b.is_empty() => b.clone(),
        _ => {
            return (StatusCode::BAD_REQUEST, "'bidder' query param is required").into_response();
        }
    };

    tracing::debug!("setuid: bidder={} uid={:?}", bidder, params.uid);

    // Parse existing cookie
    let mut cookie = UserSyncCookie::from_request(&headers, &state.host_cookie.cookie_name);

    // Set (or clear) the UID for this bidder
    if let Some(uid) = &params.uid {
        if uid.is_empty() {
            // Empty uid means opt-out / remove
            cookie.uids.remove(&bidder);
        } else {
            cookie.set_uid(&bidder, uid.clone());
        }
    }

    // Build Set-Cookie header
    let set_cookie_val = cookie.build_set_cookie_header(
        &state.host_cookie.cookie_name,
        state.host_cookie.ttl_days,
        &state.host_cookie.domain,
    );

    let format = params.f.as_deref().unwrap_or("b");
    if format == "i" {
        // Return a 1x1 tracking pixel
        let pixel = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01\x00\x00\x00\x01\x08\x02\x00\x00\x00\x90wS\xde\x00\x00\x00\x0cIDATx\x9cc\xf8\x0f\x00\x00\x01\x01\x00\x05\x18\xd8N\x00\x00\x00\x00IEND\xaeB`\x82";
        let cookie_header_val = match HeaderValue::from_str(&set_cookie_val) {
            Ok(v) => v,
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };
        let mut resp = (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "image/png")],
            pixel.to_vec(),
        )
            .into_response();
        resp.headers_mut().insert(SET_COOKIE, cookie_header_val);
        return resp;
    }

    // Blank response with cookie set
    let cookie_header_val = match HeaderValue::from_str(&set_cookie_val) {
        Ok(v) => v,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let mut resp = StatusCode::OK.into_response();
    resp.headers_mut().insert(SET_COOKIE, cookie_header_val);
    resp
}

// ──────────────────────────────────────────────────────────────────────────────
// GET /getuids
// ──────────────────────────────────────────────────────────────────────────────

pub async fn get_uids_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Json<serde_json::Value> {
    let cookie = UserSyncCookie::from_request(&headers, &state.host_cookie.cookie_name);
    let now = chrono::Utc::now().timestamp();

    // Return only non-expired UIDs, keyed by bidder name
    let buyeruids: HashMap<String, String> = cookie
        .uids
        .into_iter()
        .filter(|(_, entry)| entry.expires > now)
        .map(|(bidder, entry)| (bidder, entry.uid))
        .collect();

    Json(serde_json::json!({ "buyeruids": buyeruids }))
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
    #[serde(rename = "coopSync")]
    pub coop_sync: Option<bool>,
    #[serde(rename = "filterSettings")]
    pub filter_settings: Option<serde_json::Value>,
    pub account: Option<String>,
}

#[derive(Serialize)]
pub struct CookieSyncResponse {
    pub status: String,
    pub bidder_status: Vec<serde_json::Value>,
}

pub async fn cookie_sync_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CookieSyncRequest>,
) -> Json<CookieSyncResponse> {
    let cookie = UserSyncCookie::from_request(&headers, &state.host_cookie.cookie_name);
    let has_cookie = !cookie.uids.is_empty();

    // Determine which bidders to check — use requested list or fall back to all known
    let requested: Vec<String> = body.bidders.unwrap_or_default();

    let limit = body.limit.unwrap_or(10).max(1) as usize;

    let mut bidder_status: Vec<serde_json::Value> = Vec::new();

    let candidates: Vec<&str> = if requested.is_empty() {
        // All bidders we have sync URLs for
        vec![
            "appnexus", "openx", "pubmatic", "ix", "sovrn", "adform",
            "33across", "criteo", "yieldmo", "sharethrough", "smaato",
            "conversant", "smartadserver", "yieldlab", "triplelift",
        ]
    } else {
        requested.iter().map(|s| s.as_str()).collect()
    };

    for bidder in candidates {
        if bidder_status.len() >= limit {
            break;
        }
        // Skip if already synced and not expired
        if cookie.has_valid_uid(bidder) {
            bidder_status.push(serde_json::json!({
                "bidder": bidder,
                "no_cookie": false,
                "usersync": {}
            }));
            continue;
        }

        // Look up sync URL
        if let Some((sync_type, url)) = bidder_sync_url(bidder) {
            bidder_status.push(serde_json::json!({
                "bidder": bidder,
                "no_cookie": true,
                "usersync": {
                    "url": url,
                    "type": sync_type
                }
            }));
        }
    }

    let status = if has_cookie { "ok" } else { "no_cookie" };

    Json(CookieSyncResponse {
        status: status.to_string(),
        bidder_status,
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
