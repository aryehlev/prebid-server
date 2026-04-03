use axum::{
    extract::{Json, Path, Query, State},
    http::{header::{COOKIE, SET_COOKIE}, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use base64::{engine::general_purpose::STANDARD, Engine};
use pbs_metrics::MetricsEngine as _;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};

pub mod stored_requests;
pub use stored_requests::StoredRequestFetcher;

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
    /// Stored request fetcher for AMP and other stored-request endpoints
    pub stored_requests: Arc<StoredRequestFetcher>,
    /// Prometheus metrics engine
    pub metrics: Arc<pbs_metrics::PrometheusMetrics>,
    /// Maximum allowed request body size in bytes (0 = unlimited)
    pub max_request_size: usize,
    /// Whether GDPR enforcement is enabled
    pub gdpr_enabled: bool,
    /// Account-level configurations
    pub accounts: std::collections::HashMap<String, pbs_config::AccountConfig>,
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
// GET /health
// ──────────────────────────────────────────────────────────────────────────────

pub async fn health_handler() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status": "UP", "checks": []}))
}

// ──────────────────────────────────────────────────────────────────────────────
// GET /ready
// ──────────────────────────────────────────────────────────────────────────────

pub async fn readiness_handler(State(state): State<AppState>) -> Response {
    if state.exchange.adapter_count() == 0 {
        return (StatusCode::SERVICE_UNAVAILABLE, "no adapters").into_response();
    }
    (StatusCode::OK, Json(serde_json::json!({"status": "READY"}))).into_response()
}

// ──────────────────────────────────────────────────────────────────────────────
// POST /openrtb2/auction
// ──────────────────────────────────────────────────────────────────────────────

pub async fn auction_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(bid_request): Json<openrtb::BidRequest>,
) -> Response {
    // Check Content-Length against configured max_request_size
    if state.max_request_size > 0 {
        if let Some(cl_val) = headers.get(axum::http::header::CONTENT_LENGTH) {
            if let Ok(cl_str) = cl_val.to_str() {
                if let Ok(content_length) = cl_str.parse::<usize>() {
                    if content_length > state.max_request_size {
                        return (
                            StatusCode::BAD_REQUEST,
                            format!(
                                "request size {} exceeded max size of {} bytes",
                                content_length, state.max_request_size
                            ),
                        )
                            .into_response();
                    }
                }
            }
        }
    }

    if bid_request.id.is_empty() {
        return (StatusCode::BAD_REQUEST, "request.id is required").into_response();
    }
    if bid_request.imp.is_empty() {
        return (StatusCode::BAD_REQUEST, "request.imp must contain at least one impression").into_response();
    }

    // Look up account config from the publisher ID embedded in site.publisher.id
    let account_id = bid_request
        .site
        .as_ref()
        .and_then(|s| s.publisher.as_ref())
        .and_then(|p| p.id.as_deref());

    let account_cfg = account_id.and_then(|id| state.accounts.get(id));

    // If account requires GDPR consent and none is present, skip the auction
    if let Some(acct) = account_cfg {
        if acct.gdpr_enabled == Some(true) {
            let has_consent = bid_request
                .user
                .as_ref()
                .and_then(|u| u.ext.as_ref())
                .and_then(|e| e.get("consent"))
                .and_then(|v| v.as_str())
                .map(|s| !s.is_empty())
                .unwrap_or(false);
            if !has_consent {
                let empty_response = openrtb::BidResponse {
                    id: bid_request.id.clone(),
                    ..Default::default()
                };
                return (StatusCode::OK, Json(empty_response)).into_response();
            }
        }
    }

    // Apply account-level tmax override if set
    let mut bid_request = bid_request;
    if let Some(acct) = account_cfg {
        if let Some(tmax) = acct.auction_timeout_ms {
            bid_request.tmax = Some(tmax as i64);
        }
    }

    let auction_req = pbs_exchange::AuctionRequest {
        bid_request,
        account: None,
        user_syncs: None,
        start_time: std::time::Instant::now(),
        currency_rates: None,
    };

    match state.exchange.hold_auction(auction_req).await {
        Ok(auction_response) => {
            state.metrics.record_request("openrtb2", pbs_metrics::RequestStatus::Ok);
            (StatusCode::OK, Json(auction_response.bid_response)).into_response()
        }
        Err(e) => {
            tracing::error!("Auction error: {}", e);
            state.metrics.record_request("openrtb2", pbs_metrics::RequestStatus::BadServerResponse);
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// POST /openrtb2/video
// ──────────────────────────────────────────────────────────────────────────────

pub async fn video_auction_handler(
    State(state): State<AppState>,
    Json(mut bid_request): Json<openrtb::BidRequest>,
) -> Response {
    // Load stored request if req.ext.prebid.storedrequest.id is set, then merge.
    let stored_id = bid_request
        .ext
        .as_ref()
        .and_then(|e| e.get("prebid"))
        .and_then(|p| p.get("storedrequest"))
        .and_then(|sr| sr.get("id"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    if let Some(id) = stored_id {
        if let Some(stored) = state.stored_requests.get(&id) {
            // Merge stored request: append stored imps, use stored site/app if not set
            if let Ok(stored_req) =
                serde_json::from_value::<openrtb::BidRequest>(stored.clone())
            {
                // Merge imp arrays: append stored imps that don't already exist
                let existing_imp_ids: std::collections::HashSet<String> =
                    bid_request.imp.iter().map(|i| i.id.clone()).collect();
                for imp in stored_req.imp {
                    if !existing_imp_ids.contains(&imp.id) {
                        bid_request.imp.push(imp);
                    }
                }
                // Use stored site if incoming request doesn't have one
                if bid_request.site.is_none() {
                    bid_request.site = stored_req.site;
                }
                // Use stored app if incoming request doesn't have one
                if bid_request.app.is_none() {
                    bid_request.app = stored_req.app;
                }
            }
        }
    }

    let auction_req = pbs_exchange::AuctionRequest {
        bid_request,
        account: None,
        user_syncs: None,
        start_time: std::time::Instant::now(),
        currency_rates: None,
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
    State(state): State<AppState>,
    Query(params): Query<AmpParams>,
) -> Response {
    let tag_id = match &params.tag_id {
        Some(t) if !t.is_empty() => t.clone(),
        _ => {
            return (StatusCode::BAD_REQUEST, "AMP request missing required tag_id query parameter").into_response();
        }
    };

    // Look up the stored request by tag_id
    match state.stored_requests.get(&tag_id) {
        Some(stored) => {
            let response = serde_json::json!({
                "tag_id": tag_id,
                "targeting": {},
                "stored_request": stored,
            });
            (StatusCode::OK, Json(response)).into_response()
        }
        None => {
            let response = serde_json::json!({
                "tag_id": tag_id,
                "targeting": {},
                "errors": { "prebid": [{"code": 2, "message": format!("stored request not found for tag_id: {}", tag_id)}] }
            });
            (StatusCode::BAD_REQUEST, Json(response)).into_response()
        }
    }
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

/// All known bidder names that have sync URLs configured.
const KNOWN_SYNC_BIDDERS: &[&str] = &[
    "appnexus", "openx", "pubmatic", "ix", "sovrn", "adform",
    "33across", "criteo", "yieldmo", "sharethrough", "smaato",
    "conversant", "smartadserver", "yieldlab", "triplelift",
];

/// Build a sync URL from a template, appending GDPR params when present.
/// The `{{.RedirectURL}}` macro in templates is left as-is (server-side macro).
fn build_sync_url(template: &str, gdpr: Option<i32>, gdpr_consent: Option<&str>) -> String {
    let mut params: Vec<String> = Vec::new();
    if let Some(g) = gdpr {
        params.push(format!("gdpr={}", g));
    }
    if let Some(gc) = gdpr_consent {
        if !gc.is_empty() {
            params.push(format!("gdpr_consent={}", gc));
        }
    }
    if params.is_empty() {
        return template.to_string();
    }
    if template.contains('?') {
        format!("{}&{}", template, params.join("&"))
    } else {
        format!("{}?{}", template, params.join("&"))
    }
}

pub async fn cookie_sync_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CookieSyncRequest>,
) -> Json<CookieSyncResponse> {
    let cookie = UserSyncCookie::from_request(&headers, &state.host_cookie.cookie_name);
    let has_cookie = !cookie.uids.is_empty();

    let gdpr = body.gdpr;
    let gdpr_consent = body.gdpr_consent.clone();

    // Determine which bidders to check — use requested list or fall back to all known
    let requested: Vec<String> = body.bidders.unwrap_or_default();

    // Cap returned *new* syncs at `limit`; already-synced bidders do NOT count against it.
    let limit = body.limit.unwrap_or(10).max(1) as usize;

    let mut bidder_status: Vec<serde_json::Value> = Vec::new();
    let mut new_sync_count: usize = 0;

    // Build the candidate list, validating names against known bidders when explicitly requested.
    let all_known: std::collections::HashSet<&str> = KNOWN_SYNC_BIDDERS.iter().copied().collect();

    let candidates: Vec<String> = if requested.is_empty() {
        KNOWN_SYNC_BIDDERS.iter().map(|s| s.to_string()).collect()
    } else {
        let mut valid = Vec::new();
        for name in &requested {
            let key = name.as_str();
            if all_known.contains(key) || state.bidder_info.contains_key(key) {
                valid.push(name.clone());
            } else {
                tracing::warn!("cookie_sync: unknown bidder '{}' requested; skipping", name);
            }
        }
        valid
    };

    for bidder in &candidates {
        // Already synced — include in status but don't count against limit.
        if cookie.has_valid_uid(bidder) {
            bidder_status.push(serde_json::json!({
                "bidder": bidder,
                "no_cookie": false,
                "usersync": {}
            }));
            continue;
        }

        // Enforce limit on *new* syncs.
        if new_sync_count >= limit {
            break;
        }

        // Look up sync URL and apply GDPR params.
        if let Some((sync_type, url_template)) = bidder_sync_url(bidder) {
            let url = build_sync_url(url_template, gdpr, gdpr_consent.as_deref());
            bidder_status.push(serde_json::json!({
                "bidder": bidder,
                "no_cookie": true,
                "usersync": {
                    "url": url,
                    "type": sync_type
                }
            }));
            new_sync_count += 1;
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

impl EventParams {
    /// Validate required parameters. Returns an error message if any required field is missing or invalid.
    pub fn validate(&self) -> Result<(), String> {
        match self.event_type.as_deref() {
            None | Some("") => {
                return Err("parameter 't' is required".to_string());
            }
            Some(t) if t != "win" && t != "imp" => {
                return Err(format!("unknown type: '{}'", t));
            }
            _ => {}
        }
        match self.bid_id.as_deref() {
            None | Some("") => {
                return Err("parameter 'b' is required".to_string());
            }
            _ => {}
        }
        match self.account_id.as_deref() {
            None | Some("") => {
                return Err("parameter 'a' is required".to_string());
            }
            _ => {}
        }
        Ok(())
    }
}

pub async fn event_handler(
    State(_state): State<AppState>,
    Query(params): Query<EventParams>,
) -> Response {
    // Validate required parameters
    if let Err(msg) = params.validate() {
        return (StatusCode::BAD_REQUEST, format!("invalid request: {}", msg)).into_response();
    }

    let event_type = params.event_type.as_deref().unwrap_or("");
    let bid_id = params.bid_id.as_deref().unwrap_or("");
    let account_id = params.account_id.as_deref().unwrap_or("");
    let bidder = params.bidder.as_deref().unwrap_or("");
    let timestamp = params.timestamp.unwrap_or_else(|| chrono::Utc::now().timestamp_millis());

    // Log the event at info level
    match event_type {
        "win" => {
            tracing::info!(
                event_type = "win",
                bid_id = bid_id,
                account_id = account_id,
                bidder = bidder,
                timestamp = timestamp,
                "win notification received"
            );
        }
        "imp" => {
            tracing::info!(
                event_type = "imp",
                bid_id = bid_id,
                account_id = account_id,
                bidder = bidder,
                timestamp = timestamp,
                "impression notification received"
            );
        }
        _ => {}
    }

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

/// Inject a tracking impression URL into a VAST XML string.
///
/// Mirrors the Go `ModifyVastXmlString` logic:
/// - If there is no `</Impression>` tag, return the input unchanged.
/// - If the nearest `<Impression>` is immediately followed by `</Impression>`
///   (empty tag), insert the CDATA URL inside that empty element.
/// - Otherwise append a new `<Impression>…</Impression>` element right after
///   the first `</Impression>` closing tag.
pub fn modify_vast_xml(vast: &str, tracking_url: &str) -> String {
    const CLOSE_TAG: &str = "</Impression>";
    const OPEN_TAG: &str = "<Impression>";

    let ci = match vast.find(CLOSE_TAG) {
        Some(idx) => idx,
        None => return vast.to_string(),
    };

    let cdata = format!("<![CDATA[{}]]>", tracking_url);

    // Check whether the nearest open tag is immediately followed by the close tag
    // i.e. <Impression></Impression> — empty element.
    if let Some(oi) = vast.find(OPEN_TAG) {
        if ci - oi == OPEN_TAG.len() {
            // Insert CDATA inside the empty element
            return vast.replacen(OPEN_TAG, &format!("{}{}", OPEN_TAG, cdata), 1);
        }
    }

    // Append a new Impression element after the first closing tag
    let injection = format!("{}{}{}{}", CLOSE_TAG, OPEN_TAG, cdata, CLOSE_TAG);
    vast.replacen(CLOSE_TAG, &injection, 1)
}

/// Build the event/tracking URL from query params.
///
/// Pattern: `/event?t=imp&b={bid_id}&a={account_id}[&bidder={bidder}][&ts={ts}]`
/// `bid_id` is taken from the `b` query param; `account_id` from `a`.
fn build_vast_tracking_url(params: &HashMap<String, String>) -> String {
    let account = params.get("a").map(|s| s.as_str()).unwrap_or("");
    let bid_id = params.get("b").map(|s| s.as_str()).unwrap_or("");
    let bidder = params.get("bidder").map(|s| s.as_str()).unwrap_or("");
    let ts = params.get("ts").map(|s| s.as_str()).unwrap_or("");

    let mut url = format!("/event?t=imp&b={}&a={}", bid_id, account);
    if !bidder.is_empty() {
        url.push_str(&format!("&bidder={}", bidder));
    }
    if !ts.is_empty() {
        url.push_str(&format!("&ts={}", ts));
    }
    url
}

pub async fn vtrack_handler(
    State(_state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
    body: axum::body::Bytes,
) -> Response {
    let account = params.get("a").cloned().unwrap_or_default();
    tracing::debug!("vtrack: account={} body_len={}", account, body.len());

    // Convert body to string; if not valid UTF-8 pass through unchanged.
    let vast_str = match std::str::from_utf8(&body) {
        Ok(s) => s,
        Err(_) => {
            return (StatusCode::OK, body).into_response();
        }
    };

    // Only attempt VAST XML rewriting if the body looks like VAST XML.
    if !vast_str.contains("<VAST") {
        return (StatusCode::OK, body).into_response();
    }

    let tracking_url = build_vast_tracking_url(&params);
    let modified = modify_vast_xml(vast_str, &tracking_url);

    (
        StatusCode::OK,
        [(axum::http::header::CONTENT_TYPE, "application/xml")],
        modified,
    )
        .into_response()
}

// ──────────────────────────────────────────────────────────────────────────────
// GET / — index
// ──────────────────────────────────────────────────────────────────────────────

pub async fn index_handler() -> &'static str {
    "prebid-server (Rust port)"
}

// ──────────────────────────────────────────────────────────────────────────────
// GET /metrics — Prometheus metrics
// ──────────────────────────────────────────────────────────────────────────────

pub async fn metrics_handler(State(state): State<AppState>) -> Response {
    let body = state.metrics.gather_text();
    (
        StatusCode::OK,
        [(axum::http::header::CONTENT_TYPE, "text/plain; version=0.0.4; charset=utf-8")],
        body,
    )
        .into_response()
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
        .route("/health", axum::routing::get(health_handler))
        .route("/ready", axum::routing::get(readiness_handler))
        .route("/metrics", axum::routing::get(metrics_handler))
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

// ──────────────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::util::ServiceExt as _;

    /// Build a minimal AppState suitable for unit tests.
    fn test_state() -> AppState {
        use std::sync::Arc;
        let metrics = pbs_metrics::PrometheusMetrics::new("test_endpoints")
            .expect("failed to create metrics");
        let exchange = pbs_exchange::Exchange::new(std::collections::HashMap::new());
        Arc::new(AppStateInner {
            exchange,
            version: "test".to_string(),
            revision: "abc123".to_string(),
            bidder_info: HashMap::new(),
            bidder_params: HashMap::new(),
            host_cookie: HostCookieConfig::default(),
            status_response: None,
            stored_requests: Arc::new(StoredRequestFetcher::empty()),
            metrics: Arc::new(metrics),
            max_request_size: 0,
            gdpr_enabled: false,
            accounts: std::collections::HashMap::new(),
        })
    }

    /// Send a one-shot request through the router and return `(status, body_bytes)`.
    async fn send(router: axum::Router, req: Request<Body>) -> (StatusCode, bytes::Bytes) {
        let resp = router
            .oneshot(req)
            .await
            .expect("oneshot failed");
        let status = resp.status();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("body read failed");
        (status, body)
    }

    #[tokio::test]
    async fn test_status_returns_200() {
        let router = create_router(test_state());
        let req = Request::builder()
            .method("GET")
            .uri("/status")
            .body(Body::empty())
            .unwrap();
        let (status, _body) = send(router, req).await;
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn test_version_returns_json_with_version_field() {
        let router = create_router(test_state());
        let req = Request::builder()
            .method("GET")
            .uri("/version")
            .body(Body::empty())
            .unwrap();
        let (status, body) = send(router, req).await;
        assert_eq!(status, StatusCode::OK);
        let json: serde_json::Value =
            serde_json::from_slice(&body).expect("response is not valid JSON");
        assert!(
            json.get("version").is_some(),
            "response JSON missing 'version' field: {}",
            json
        );
    }

    #[tokio::test]
    async fn test_auction_empty_body_returns_400() {
        let router = create_router(test_state());
        let req = Request::builder()
            .method("POST")
            .uri("/openrtb2/auction")
            .header("content-type", "application/json")
            .body(Body::empty())
            .unwrap();
        let (status, _body) = send(router, req).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_metrics_returns_prometheus_text() {
        let router = create_router(test_state());
        let req = Request::builder()
            .method("GET")
            .uri("/metrics")
            .body(Body::empty())
            .unwrap();
        let (status, body) = send(router, req).await;
        assert_eq!(status, StatusCode::OK);
        // Verify the body is valid UTF-8 text (Prometheus format)
        let _body_str = std::str::from_utf8(&body).expect("non-UTF-8 metrics body");
    }

    // ── Unit tests for VAST XML injection ────────────────────────────────────

    #[test]
    fn test_modify_vast_xml_no_impression_tag_unchanged() {
        let vast = r#"<VAST version="2.0"><Ad></Ad></VAST>"#;
        let result = modify_vast_xml(vast, "https://example.com/track");
        assert_eq!(result, vast);
    }

    #[test]
    fn test_modify_vast_xml_empty_impression_injects_cdata() {
        let vast = r#"<VAST><Ad><Impression></Impression></Ad></VAST>"#;
        let result = modify_vast_xml(vast, "https://example.com/track");
        assert!(result.contains("<![CDATA[https://example.com/track]]>"));
        assert!(result.contains("<Impression><![CDATA[https://example.com/track]]></Impression>"));
    }

    #[test]
    fn test_modify_vast_xml_non_empty_impression_appends_new_element() {
        let vast = r#"<VAST><Ad><Impression><![CDATA[https://existing.com]]></Impression></Ad></VAST>"#;
        let result = modify_vast_xml(vast, "https://new.com/track");
        assert!(result.contains("<![CDATA[https://existing.com]]>"));
        assert!(result.contains("<![CDATA[https://new.com/track]]>"));
        assert!(result.contains(
            "</Impression><Impression><![CDATA[https://new.com/track]]></Impression>"
        ));
    }

    // ── Unit tests for EventParams validation ────────────────────────────────

    #[test]
    fn test_event_params_missing_type_returns_error() {
        let p = EventParams {
            event_type: None,
            bid_id: Some("bid1".to_string()),
            account_id: Some("acct1".to_string()),
            bidder: None,
            format: None,
            timestamp: None,
            analytics: None,
        };
        assert!(p.validate().is_err());
    }

    #[test]
    fn test_event_params_unknown_type_returns_error() {
        let p = EventParams {
            event_type: Some("unknown".to_string()),
            bid_id: Some("bid1".to_string()),
            account_id: Some("acct1".to_string()),
            bidder: None,
            format: None,
            timestamp: None,
            analytics: None,
        };
        assert!(p.validate().is_err());
    }

    #[test]
    fn test_event_params_missing_bid_id_returns_error() {
        let p = EventParams {
            event_type: Some("win".to_string()),
            bid_id: None,
            account_id: Some("acct1".to_string()),
            bidder: None,
            format: None,
            timestamp: None,
            analytics: None,
        };
        assert!(p.validate().is_err());
    }

    #[test]
    fn test_event_params_missing_account_returns_error() {
        let p = EventParams {
            event_type: Some("imp".to_string()),
            bid_id: Some("bid1".to_string()),
            account_id: None,
            bidder: None,
            format: None,
            timestamp: None,
            analytics: None,
        };
        assert!(p.validate().is_err());
    }

    #[test]
    fn test_event_params_valid_win() {
        let p = EventParams {
            event_type: Some("win".to_string()),
            bid_id: Some("bid1".to_string()),
            account_id: Some("acct1".to_string()),
            bidder: None,
            format: None,
            timestamp: None,
            analytics: None,
        };
        assert!(p.validate().is_ok());
    }

    #[test]
    fn test_event_params_valid_imp() {
        let p = EventParams {
            event_type: Some("imp".to_string()),
            bid_id: Some("bid1".to_string()),
            account_id: Some("acct1".to_string()),
            bidder: None,
            format: None,
            timestamp: None,
            analytics: None,
        };
        assert!(p.validate().is_ok());
    }
}
