use axum::{
    extract::{Json, Path, Query, State},
    http::{header::{COOKIE, SET_COOKIE}, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use base64::{engine::general_purpose::STANDARD, Engine};
use pbs_exchange::privacy::{Activity, ActivityComponent, ActivityControl};
use pbs_exchange::usersync::{PrebidCookie, Syncer, SyncType};
use pbs_exchange::validation::ValidationError;
use pbs_metrics::MetricsEngine as _;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};

pub mod stored_requests;
pub use stored_requests::StoredRequestFetcher;

/// Sync URL info loaded from a bidder's YAML userSync section.
#[derive(Debug, Clone, Default)]
pub struct BidderSyncInfo {
    pub iframe_url: Option<String>,
    pub redirect_url: Option<String>,
}

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
    /// Bidder usersync info loaded from YAML: bidder name -> sync URLs
    pub bidder_sync_info: HashMap<String, BidderSyncInfo>,
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
    /// Optional shared currency converter for the /currency/rates endpoint
    pub currency_converter: Option<Arc<pbs_exchange::currency::CurrencyConverter>>,
    /// Whether an account ID is required for cookie_sync / setuid requests
    pub account_required: bool,
    /// Activity control for privacy enforcement (SyncUser, etc.)
    pub activity_control: ActivityControl,
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
// Cookie helper
// ──────────────────────────────────────────────────────────────────────────────

/// Extract a named cookie value from the `Cookie` request header.
/// Returns `None` if the header is missing or the named cookie is not found.
fn extract_cookie_value(headers: &HeaderMap, cookie_name: &str) -> Option<String> {
    let name = if cookie_name.is_empty() { DEFAULT_COOKIE_NAME } else { cookie_name };
    let cookie_hdr = headers.get(COOKIE)?;
    let cookie_str = cookie_hdr.to_str().ok()?;
    for pair in cookie_str.split(';') {
        let pair = pair.trim();
        if let Some((k, v)) = pair.split_once('=') {
            if k.trim() == name {
                return Some(v.trim().to_string());
            }
        }
    }
    None
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
// Request validation helper
// ──────────────────────────────────────────────────────────────────────────────

/// Validate a BidRequest and return a descriptive error string on fatal errors.
///
/// Checks that `request.id` is present, then delegates to the structured
/// validation module for OpenRTB-level rules (NoImps, BothSiteAndApp, etc.).
fn validate_bid_request(req: &openrtb::BidRequest) -> Result<(), String> {
    // Always require a non-empty request ID
    if req.id.is_empty() {
        return Err("request missing required field: request.id".to_string());
    }
    let errors = pbs_exchange::validation::validate_request(req);
    let fatal = errors.iter().any(|e| {
        matches!(e, ValidationError::NoImps | ValidationError::BothSiteAndApp)
    });
    if fatal {
        Err(errors.iter().map(|e| e.to_string()).collect::<Vec<_>>().join("; "))
    } else {
        Ok(())
    }
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

    if let Err(msg) = validate_bid_request(&bid_request) {
        return (StatusCode::BAD_REQUEST, msg).into_response();
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
        if acct.privacy.gdpr.enabled == Some(true) {
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

    // Check for stored auction response short-circuit.
    // If req.ext.prebid.storedauctionresponse.id is set, return the stored
    // BidResponse directly without running the auction.
    if let Some(stored_resp_id) = bid_request
        .ext
        .as_ref()
        .and_then(|e| e.get("prebid"))
        .and_then(|p| p.get("storedauctionresponse"))
        .and_then(|s| s.get("id"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
    {
        if let Some(mut stored_response) = state.stored_requests.fetch_stored_response(&stored_resp_id) {
            if stored_response.id.is_empty() {
                stored_response.id = bid_request.id.clone();
            }
            tracing::debug!(id = %stored_resp_id, "returning stored auction response; skipping auction");
            return (StatusCode::OK, Json(stored_response)).into_response();
        }
    }

    // Apply account-level tmax override if set
    let mut bid_request = bid_request;
    if let Some(acct) = account_cfg {
        if let Some(tmax) = acct.auction_timeout_ms {
            bid_request.tmax = Some(tmax as i64);
        }
    }

    // Resolve imp-level stored requests before running the auction.
    resolve_imp_stored_requests(&mut bid_request, &state.stored_requests);

    // Process interstitial impressions (expand banner formats for instl=1).
    process_interstitials(&mut bid_request);

    let handler_start = std::time::Instant::now();
    let auction_req = pbs_exchange::AuctionRequest {
        bid_request,
        account: None,
        user_syncs: None,
        start_time: handler_start,
        currency_rates: None,
    };

    match state.exchange.hold_auction(auction_req).await {
        Ok(auction_response) => {
            let duration_ms = handler_start.elapsed().as_millis() as u64;
            state.metrics.record_request_by_type("openrtb2", pbs_metrics::RequestStatus::Ok);
            state.metrics.record_http_request("openrtb2", 200, duration_ms);
            (StatusCode::OK, Json(auction_response.bid_response)).into_response()
        }
        Err(e) => {
            let duration_ms = handler_start.elapsed().as_millis() as u64;
            tracing::error!("Auction error: {}", e);
            state.metrics.record_request_by_type("openrtb2", pbs_metrics::RequestStatus::BadServerResponse);
            state.metrics.record_http_request("openrtb2", 500, duration_ms);
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// GET /openrtb2/auction
// ──────────────────────────────────────────────────────────────────────────────

pub async fn auction_get_handler(
    State(state): State<AppState>,
    _headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let body = match params.get("request") {
        Some(r) => r.clone(),
        None => return (StatusCode::BAD_REQUEST, "missing request parameter").into_response(),
    };

    let bid_request: openrtb::BidRequest = match serde_json::from_str(&body) {
        Ok(r) => r,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, format!("invalid request JSON: {}", e)).into_response();
        }
    };

    // Reuse same validation and auction logic as auction_handler
    if let Err(msg) = validate_bid_request(&bid_request) {
        return (StatusCode::BAD_REQUEST, msg).into_response();
    }

    let account_id = bid_request
        .site
        .as_ref()
        .and_then(|s| s.publisher.as_ref())
        .and_then(|p| p.id.as_deref());

    let account_cfg = account_id.and_then(|id| state.accounts.get(id));

    if let Some(acct) = account_cfg {
        if acct.privacy.gdpr.enabled == Some(true) {
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

    let mut bid_request = bid_request;
    if let Some(acct) = account_cfg {
        if let Some(tmax) = acct.auction_timeout_ms {
            bid_request.tmax = Some(tmax as i64);
        }
    }

    // Resolve imp-level stored requests before running the auction.
    resolve_imp_stored_requests(&mut bid_request, &state.stored_requests);

    let handler_start = std::time::Instant::now();
    let auction_req = pbs_exchange::AuctionRequest {
        bid_request,
        account: None,
        user_syncs: None,
        start_time: handler_start,
        currency_rates: None,
    };

    match state.exchange.hold_auction(auction_req).await {
        Ok(auction_response) => {
            let duration_ms = handler_start.elapsed().as_millis() as u64;
            state.metrics.record_request_by_type("openrtb2", pbs_metrics::RequestStatus::Ok);
            state.metrics.record_http_request("openrtb2", 200, duration_ms);
            (StatusCode::OK, Json(auction_response.bid_response)).into_response()
        }
        Err(e) => {
            let duration_ms = handler_start.elapsed().as_millis() as u64;
            tracing::error!("Auction error: {}", e);
            state.metrics.record_request_by_type("openrtb2", pbs_metrics::RequestStatus::BadServerResponse);
            state.metrics.record_http_request("openrtb2", 500, duration_ms);
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

    // Pod/slot deduplication: remove duplicate imp IDs, keeping first occurrence.
    {
        let mut seen_ids = std::collections::HashSet::new();
        bid_request.imp.retain(|imp| seen_ids.insert(imp.id.clone()));
    }

    // Validate the (possibly merged and deduplicated) request.
    if let Err(msg) = validate_bid_request(&bid_request) {
        return (StatusCode::BAD_REQUEST, msg).into_response();
    }

    let auction_req = pbs_exchange::AuctionRequest {
        bid_request,
        account: None,
        user_syncs: None,
        start_time: std::time::Instant::now(),
        currency_rates: None,
    };

    match state.exchange.hold_auction(auction_req).await {
        Ok(mut r) => {
            // Inject adpod targeting (hb_pb_cat_dur) for video bids.
            // For each bid in each seatbid that corresponds to a video impression,
            // add hb_pb_cat_dur targeting in the bid's ext.prebid.targeting map.
            for seatbid in &mut r.bid_response.seatbid {
                for bid in &mut seatbid.bid {
                    // Build hb_pb_cat_dur value: "<price_bucket>_<category>_<duration>s"
                    // Price bucket: floor price to 2 decimal places
                    let price_bucket = format!("{:.2}", bid.price);
                    // Extract category from bid.cat if present
                    let category = bid
                        .cat
                        .as_ref()
                        .and_then(|cats| cats.first())
                        .map(|s| s.as_str())
                        .unwrap_or("unknown");
                    // Extract duration from bid ext if available, else default to 0
                    let duration = bid
                        .ext
                        .as_ref()
                        .and_then(|e| e.get("prebid"))
                        .and_then(|p| p.get("video"))
                        .and_then(|v| v.get("duration"))
                        .and_then(|d| d.as_i64())
                        .unwrap_or(0);
                    let hb_pb_cat_dur = format!("{}_{}_{duration}s", price_bucket, category);

                    // Merge into bid.ext.prebid.targeting
                    let ext = bid.ext.get_or_insert_with(|| serde_json::json!({}));
                    let prebid = ext
                        .as_object_mut()
                        .and_then(|m| {
                            if !m.contains_key("prebid") {
                                m.insert("prebid".to_string(), serde_json::json!({}));
                            }
                            m.get_mut("prebid")
                        });
                    if let Some(prebid_obj) = prebid {
                        let targeting = prebid_obj
                            .as_object_mut()
                            .and_then(|m| {
                                if !m.contains_key("targeting") {
                                    m.insert("targeting".to_string(), serde_json::json!({}));
                                }
                                m.get_mut("targeting")
                            });
                        if let Some(t) = targeting {
                            if let Some(t_map) = t.as_object_mut() {
                                t_map.insert(
                                    "hb_pb_cat_dur".to_string(),
                                    serde_json::Value::String(hb_pb_cat_dur),
                                );
                            }
                        }
                    }
                }
            }
            (StatusCode::OK, Json(r.bid_response)).into_response()
        }
        Err(e) => {
            tracing::error!("Video auction error: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Imp-level stored request resolution
// ──────────────────────────────────────────────────────────────────────────────

/// Resolve imp-level stored requests for every imp in `bid_request.imp`.
///
/// For each imp whose `imp.ext.prebid.storedrequest.id` is set, fetch the
/// stored imp fragment via `StoredRequestFetcher::fetch_imp` and deep-merge it
/// into the imp (the incoming imp's fields win on conflict, matching the Go
/// server's `jsonpatch.MergePatch(stored, incoming)` semantics).
fn resolve_imp_stored_requests(
    bid_request: &mut openrtb::BidRequest,
    stored_requests: &StoredRequestFetcher,
) {
    for imp in &mut bid_request.imp {
        let stored_imp_id = imp
            .ext
            .as_ref()
            .and_then(|e| e.get("prebid"))
            .and_then(|p| p.get("storedrequest"))
            .and_then(|sr| sr.get("id"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        if let Some(id) = stored_imp_id {
            if let Some(stored_imp_json) = stored_requests.fetch_imp(&id) {
                // Serialize the current imp to JSON, merge the stored fragment
                // as the base (stored values fill in missing fields), then
                // deserialize back.
                let mut imp_value = match serde_json::to_value(&*imp) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                StoredRequestFetcher::merge_request_fragment(stored_imp_json, &mut imp_value);
                if let Ok(merged_imp) = serde_json::from_value::<openrtb::Imp>(imp_value) {
                    *imp = merged_imp;
                }
            } else {
                tracing::warn!(imp_id = %imp.id, stored_imp_id = %id, "stored imp not found");
            }
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
            return (
                StatusCode::BAD_REQUEST,
                "AMP request missing required tag_id query parameter",
            )
                .into_response();
        }
    };

    // ── 1. Load stored request by tag_id ─────────────────────────────────────
    let stored_json = match state.stored_requests.get(&tag_id) {
        Some(v) => v.clone(),
        None => {
            let response = serde_json::json!({
                "targeting": {},
                "errors": {
                    "prebid": [{
                        "code": 2,
                        "message": format!("stored request not found for tag_id: {}", tag_id)
                    }]
                }
            });
            return (StatusCode::BAD_REQUEST, Json(response)).into_response();
        }
    };

    // ── 2. Deserialise the stored BidRequest fragment ─────────────────────────
    let mut bid_request: openrtb::BidRequest =
        match serde_json::from_value(stored_json) {
            Ok(r) => r,
            Err(e) => {
                let response = serde_json::json!({
                    "targeting": {},
                    "errors": {
                        "prebid": [{
                            "code": 3,
                            "message": format!("stored request for tag_id '{}' is not a valid BidRequest: {}", tag_id, e)
                        }]
                    }
                });
                return (StatusCode::BAD_REQUEST, Json(response)).into_response();
            }
        };

    // Ensure request has an ID (use tag_id if absent).
    if bid_request.id.is_empty() {
        bid_request.id = tag_id.clone();
    }

    // ── 2b. Resolve imp-level stored requests ────────────────────────────────
    resolve_imp_stored_requests(&mut bid_request, &state.stored_requests);

    // ── 3. Apply query-param overrides ────────────────────────────────────────
    // curl → site.page (canonical URL of the AMP page)
    if let Some(curl) = params.curl.as_deref().filter(|s| !s.is_empty()) {
        let site = bid_request.site.get_or_insert_with(Default::default);
        if site.page.is_none() {
            site.page = Some(curl.to_string());
        }
    }

    // w / h → override banner size on the first impression's banner object.
    if params.w.is_some() || params.h.is_some() {
        if let Some(imp) = bid_request.imp.first_mut() {
            let banner = imp.banner.get_or_insert_with(Default::default);
            if let Some(w) = params.w {
                banner.w = Some(w);
            }
            if let Some(h) = params.h {
                banner.h = Some(h);
            }
        }
    }

    // ms (multi-size) → imp[0].banner.format
    // Format: "WxH,WxH,..." e.g. "300x250,728x90"
    if let Some(ms) = params.ms.as_deref().filter(|s| !s.is_empty()) {
        let mut formats: Vec<openrtb::Format> = Vec::new();
        for size_str in ms.split(',') {
            let size_str = size_str.trim();
            if let Some((w_str, h_str)) = size_str.split_once('x') {
                if let (Ok(w), Ok(h)) = (w_str.trim().parse::<i32>(), h_str.trim().parse::<i32>()) {
                    if w > 0 && h > 0 {
                        formats.push(openrtb::Format {
                            w: Some(w),
                            h: Some(h),
                            ..Default::default()
                        });
                    }
                }
            }
        }
        if !formats.is_empty() {
            if let Some(imp) = bid_request.imp.first_mut() {
                let banner = imp.banner.get_or_insert_with(Default::default);
                banner.format = Some(formats);
            }
        }
    }

    // Apply interstitial processing for instl=1 impressions.
    process_interstitials(&mut bid_request);

    // slot → imp[0].tagid
    if let Some(slot) = params.slot.as_deref().filter(|s| !s.is_empty()) {
        if let Some(imp) = bid_request.imp.first_mut() {
            if imp.tagid.is_none() {
                imp.tagid = Some(slot.to_string());
            }
        }
    }

    // gdpr_consent / gdpr_applies → user.ext.consent and regs.ext.gdpr
    if let Some(consent) = params.gdpr_consent.as_deref().filter(|s| !s.is_empty()) {
        let user = bid_request.user.get_or_insert_with(Default::default);
        let ext = user.ext.get_or_insert_with(|| serde_json::json!({}));
        if ext.get("consent").is_none() {
            ext["consent"] = serde_json::Value::String(consent.to_string());
        }
    }
    if let Some(gdpr) = params.gdpr_applies {
        let regs = bid_request.regs.get_or_insert_with(Default::default);
        let ext = regs.ext.get_or_insert_with(|| serde_json::json!({}));
        if ext.get("gdpr").is_none() {
            ext["gdpr"] = serde_json::Value::Number(if gdpr { 1.into() } else { 0.into() });
        }
    }

    // us_privacy → regs.us_privacy
    if let Some(usp) = params.us_privacy.as_deref().filter(|s| !s.is_empty()) {
        let regs = bid_request.regs.get_or_insert_with(Default::default);
        if regs.us_privacy.is_none() {
            regs.us_privacy = Some(usp.to_string());
        }
    }

    // account → site.publisher.id
    if let Some(account) = params.account.as_deref().filter(|s| !s.is_empty()) {
        let site = bid_request.site.get_or_insert_with(Default::default);
        let pub_ = site.publisher.get_or_insert_with(Default::default);
        if pub_.id.is_none() {
            pub_.id = Some(account.to_string());
        }
    }

    // ── 4. Run the auction ────────────────────────────────────────────────────
    let handler_start = std::time::Instant::now();
    let auction_req = pbs_exchange::AuctionRequest {
        bid_request,
        account: None,
        user_syncs: None,
        start_time: handler_start,
        currency_rates: None,
    };

    let auction_response = match state.exchange.hold_auction(auction_req).await {
        Ok(r) => r,
        Err(e) => {
            let duration_ms = handler_start.elapsed().as_millis() as u64;
            tracing::error!(tag_id = %tag_id, "AMP auction error: {}", e);
            state.metrics.record_http_request("amp", 500, duration_ms);
            let response = serde_json::json!({
                "targeting": {},
                "errors": {
                    "prebid": [{"code": 999, "message": format!("auction error: {}", e)}]
                }
            });
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(response)).into_response();
        }
    };

    // ── 5. Collect targeting keys from the winning bid ─────────────────────────
    // Flatten all per-impression targeting maps into a single map.
    // For AMP, targeting keys from the auction's first/only impression are used.
    // If the exchange computed winner keys (hb_pb, hb_bidder, hb_adid), they are
    // already in `auction_response.targeting`.
    let mut flat_targeting: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();

    for (_imp_id, keys) in auction_response.targeting {
        for (k, v) in keys {
            flat_targeting.entry(k).or_insert(v);
        }
    }

    let duration_ms = handler_start.elapsed().as_millis() as u64;
    state.metrics.record_request_by_type("amp", pbs_metrics::RequestStatus::Ok);
    state.metrics.record_http_request("amp", 200, duration_ms);

    let response = serde_json::json!({ "targeting": flat_targeting });
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

    // ── Account validation ───────────────────────────────────────────────────
    if state.account_required {
        match &params.account {
            Some(acct_id) if !acct_id.is_empty() => {
                if !state.accounts.contains_key(acct_id) {
                    return (
                        StatusCode::BAD_REQUEST,
                        format!("Invalid account '{}': account not found", acct_id),
                    )
                        .into_response();
                }
            }
            _ => {
                return (
                    StatusCode::BAD_REQUEST,
                    "Account is required but was not provided",
                )
                    .into_response();
            }
        }
    }

    // ── GDPR enforcement ─────────────────────────────────────────────────────
    // When GDPR applies (gdpr=1) and no valid consent string is present,
    // block the sync and return 451 Unavailable For Legal Reasons.
    if state.gdpr_enabled {
        let gdpr_signal = params.gdpr.unwrap_or(0);
        let consent = params.gdpr_consent.as_deref().unwrap_or("");
        if gdpr_signal == 1 && consent.is_empty() {
            return (
                StatusCode::UNAVAILABLE_FOR_LEGAL_REASONS,
                "The gdpr_consent string prevents cookies from being saved",
            )
                .into_response();
        }
    }

    // Parse existing cookie using PrebidCookie from exchange usersync module
    let raw_cookie_val = extract_cookie_value(&headers, &state.host_cookie.cookie_name);
    let mut prebid_cookie = raw_cookie_val
        .as_deref()
        .map(PrebidCookie::from_cookie)
        .unwrap_or_default();

    // Also parse via UserSyncCookie for Set-Cookie building (TTL-aware)
    let mut cookie = UserSyncCookie::from_request(&headers, &state.host_cookie.cookie_name);

    // Set (or clear) the UID for this bidder in both cookie representations
    if let Some(uid) = &params.uid {
        if uid.is_empty() {
            // Empty uid means opt-out / remove
            cookie.uids.remove(&bidder);
            prebid_cookie.uids.remove(&bidder);
        } else {
            cookie.set_uid(&bidder, uid.clone());
            prebid_cookie.set_uid(bidder.clone(), uid.clone());
        }
    }

    tracing::debug!(
        "setuid: gdpr={:?} gdpr_consent={:?} prebid_cookie_uids={}",
        params.gdpr,
        params.gdpr_consent,
        prebid_cookie.uids.len()
    );

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

/// Sync type filter modes, mirroring Go's `usersync.BidderFilterMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BidderFilterMode {
    Include,
    Exclude,
}

/// A bidder filter that either uniformly allows/blocks all bidders, or
/// applies to a specific set.
#[derive(Debug, Clone)]
pub enum BidderFilter {
    /// Applies the mode to all bidders (the `"*"` wildcard).
    Uniform(BidderFilterMode),
    /// Applies the mode to only the listed bidders.
    Specific {
        bidders: Vec<String>,
        mode: BidderFilterMode,
    },
}

impl BidderFilter {
    /// Returns true if the given bidder is allowed by this filter.
    pub fn allows(&self, bidder: &str) -> bool {
        match self {
            BidderFilter::Uniform(BidderFilterMode::Include) => true,
            BidderFilter::Uniform(BidderFilterMode::Exclude) => false,
            BidderFilter::Specific { bidders, mode } => {
                let found = bidders.iter().any(|b| b.eq_ignore_ascii_case(bidder));
                match mode {
                    BidderFilterMode::Include => found,
                    BidderFilterMode::Exclude => !found,
                }
            }
        }
    }
}

/// Per-sync-type filter, mirroring Go's `usersync.SyncTypeFilter`.
#[derive(Debug, Clone)]
pub struct SyncTypeFilter {
    pub iframe: BidderFilter,
    pub redirect: BidderFilter,
}

impl Default for SyncTypeFilter {
    fn default() -> Self {
        Self {
            iframe: BidderFilter::Uniform(BidderFilterMode::Include),
            redirect: BidderFilter::Uniform(BidderFilterMode::Include),
        }
    }
}

impl SyncTypeFilter {
    /// Return allowed sync types for a given bidder.
    pub fn allowed_types(&self, bidder: &str) -> Vec<&'static str> {
        let mut types = Vec::new();
        if self.iframe.allows(bidder) {
            types.push("iframe");
        }
        if self.redirect.allows(bidder) {
            types.push("redirect");
        }
        types
    }
}

/// Parse a single filter object from JSON (`{ "bidders": ..., "filter": "include"|"exclude" }`).
fn parse_bidder_filter(filter: &serde_json::Value) -> Result<BidderFilter, String> {
    let mode_str = filter
        .get("filter")
        .and_then(|v| v.as_str())
        .unwrap_or("include");
    let mode = match mode_str {
        "include" => BidderFilterMode::Include,
        "exclude" => BidderFilterMode::Exclude,
        other => {
            return Err(format!(
                "invalid filter value '{}'. must be either 'include' or 'exclude'",
                other
            ))
        }
    };

    match filter.get("bidders") {
        None => Ok(BidderFilter::Uniform(mode)),
        Some(serde_json::Value::String(s)) if s == "*" => Ok(BidderFilter::Uniform(mode)),
        Some(serde_json::Value::String(s)) => Err(format!(
            "invalid bidders value `{}`. must either be '*' or a string array",
            s
        )),
        Some(serde_json::Value::Array(arr)) => {
            let mut bidders = Vec::with_capacity(arr.len());
            for v in arr {
                match v.as_str() {
                    Some(b) => bidders.push(b.to_string()),
                    None => {
                        return Err(
                            "invalid bidders type. must either be a string '*' or a string array of bidders"
                                .to_string(),
                        )
                    }
                }
            }
            Ok(BidderFilter::Specific { bidders, mode })
        }
        Some(_) => Err(
            "invalid bidders type. must either be a string '*' or a string array of bidders"
                .to_string(),
        ),
    }
}

/// Parse the `filterSettings` object into a `SyncTypeFilter`.
/// Go uses `iframe` for iframes and `image` for redirects.
fn parse_type_filter(filter_settings: Option<&serde_json::Value>) -> Result<SyncTypeFilter, String> {
    let mut stf = SyncTypeFilter::default();
    let settings = match filter_settings {
        Some(v) if v.is_object() => v,
        Some(_) => return Err("filterSettings must be an object".to_string()),
        None => return Ok(stf),
    };

    if let Some(iframe) = settings.get("iframe") {
        stf.iframe = parse_bidder_filter(iframe)
            .map_err(|e| format!("error parsing filtersettings.iframe: {}", e))?;
    }
    // Go uses "image" as the JSON key for redirect filters
    if let Some(image) = settings.get("image") {
        stf.redirect = parse_bidder_filter(image)
            .map_err(|e| format!("error parsing filtersettings.image: {}", e))?;
    }
    Ok(stf)
}

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
    pub debug: Option<bool>,
    pub gpp: Option<String>,
    pub gpp_sid: Option<String>,
}

#[derive(Serialize)]
pub struct CookieSyncResponse {
    pub status: String,
    pub bidder_status: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug: Option<Vec<CookieSyncDebugEntry>>,
}

#[derive(Serialize, Clone)]
pub struct CookieSyncDebugEntry {
    pub bidder: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// All known bidder names that have sync URLs configured.
const KNOWN_SYNC_BIDDERS: &[&str] = &[
    "appnexus", "openx", "pubmatic", "ix", "sovrn", "adform",
    "33across", "criteo", "yieldmo", "sharethrough", "smaato",
    "conversant", "smartadserver", "yieldlab", "triplelift",
];

/// Compute the effective limit for cookie sync, applying account-level
/// default_limit and max_limit, mirroring Go's `setLimit`.
fn compute_effective_limit(
    request_limit: Option<i32>,
    account_default_limit: Option<i32>,
    account_max_limit: Option<i32>,
) -> usize {
    let limit = request_limit
        .or(account_default_limit)
        .filter(|&l| l > 0)
        .map(|l| l as usize)
        .unwrap_or(usize::MAX);

    let max_limit = account_max_limit
        .filter(|&m| m > 0)
        .map(|m| m as usize)
        .unwrap_or(usize::MAX);

    limit.min(max_limit)
}

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

/// Build a Set-Cookie value with the `Partitioned` attribute for CHIPS support.
/// Mirrors Go's `setCookiePartitioned`.
fn build_partitioned_set_cookie(base_cookie_val: &str) -> String {
    format!("{}; Partitioned", base_cookie_val)
}

/// Detect Chrome version from User-Agent string for SameSite workarounds.
/// Chrome >= 67 requires SameSite=None cookies to also be Secure.
/// Mirrors Go's `siteCookieCheck` + `checkChromeBrowserVersion`.
fn is_chrome_needing_samesite(user_agent: &str) -> bool {
    const CHROME_STR: &str = "Chrome/";
    const CRIOS_STR: &str = "CriOS/";
    const CHROME_MIN_VER: i32 = 67;

    fn check_version(ua: &str, prefix: &str) -> bool {
        if let Some(idx) = ua.find(prefix) {
            let version_start = idx + prefix.len();
            let remaining = &ua[version_start..];
            let dot_idx = remaining.find('.').unwrap_or(remaining.len());
            if let Ok(ver) = remaining[..dot_idx].parse::<i32>() {
                return ver >= CHROME_MIN_VER;
            }
        }
        false
    }

    check_version(user_agent, CHROME_STR) || check_version(user_agent, CRIOS_STR)
}

/// Determine the best sync type for a bidder given the type filter and
/// available sync info. Returns `None` if all sync types are filtered out.
fn choose_sync_type<'a>(
    bidder: &str,
    sync_info: Option<&BidderSyncInfo>,
    type_filter: &SyncTypeFilter,
) -> Option<(&'static str, bool)> {
    // Get the available types for this bidder
    let has_iframe = sync_info
        .map(|s| s.iframe_url.is_some())
        .unwrap_or(false)
        || bidder_sync_url(bidder).map(|(t, _)| t == "iframe").unwrap_or(false);
    let has_redirect = sync_info
        .map(|s| s.redirect_url.is_some())
        .unwrap_or(false)
        || bidder_sync_url(bidder).map(|(t, _)| t == "redirect").unwrap_or(false);

    let iframe_allowed = type_filter.iframe.allows(bidder);
    let redirect_allowed = type_filter.redirect.allows(bidder);

    // Prefer redirect over iframe (matching Go behavior), but respect the filter
    if has_redirect && redirect_allowed {
        Some(("redirect", true))
    } else if has_iframe && iframe_allowed {
        Some(("iframe", true))
    } else if has_redirect && !redirect_allowed {
        Some(("redirect", false)) // has it but filtered
    } else if has_iframe && !iframe_allowed {
        Some(("iframe", false)) // has it but filtered
    } else {
        None
    }
}

pub async fn cookie_sync_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CookieSyncRequest>,
) -> Response {
    let debug_enabled = body.debug.unwrap_or(false);

    // ── Account validation ───────────────────────────────────────────────────
    let account_id = body.account.as_deref().unwrap_or("");
    let account_cfg = if !account_id.is_empty() {
        state.accounts.get(account_id)
    } else {
        None
    };

    if state.account_required {
        if account_id.is_empty() {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "status": "error",
                    "bidder_status": [],
                    "error": "account must be valid if provided, please reach out to the prebid server host"
                })),
            )
                .into_response();
        }
        if !account_id.is_empty() && !state.accounts.contains_key(account_id) {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "status": "error",
                    "bidder_status": [],
                    "error": "account is disabled, please reach out to the prebid server host"
                })),
            )
                .into_response();
        }
    }

    // ── GDPR enforcement ─────────────────────────────────────────────────────
    if state.gdpr_enabled {
        let gdpr_signal = body.gdpr.unwrap_or(0);
        let consent = body.gdpr_consent.as_deref().unwrap_or("");
        if gdpr_signal == 1 && consent.is_empty() {
            return (
                StatusCode::UNAVAILABLE_FOR_LEGAL_REASONS,
                Json(serde_json::json!({
                    "status": "error",
                    "bidder_status": [],
                    "error": "gdpr_consent is required if gdpr=1"
                })),
            )
                .into_response();
        }
    }

    // ── Parse type filter from filterSettings ────────────────────────────────
    let type_filter = match parse_type_filter(body.filter_settings.as_ref()) {
        Ok(tf) => tf,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "status": "error",
                    "bidder_status": [],
                    "error": e
                })),
            )
                .into_response();
        }
    };

    // Parse cookie using PrebidCookie from exchange usersync module
    let raw_cookie_val = extract_cookie_value(&headers, &state.host_cookie.cookie_name);
    let prebid_cookie = raw_cookie_val
        .as_deref()
        .map(PrebidCookie::from_cookie)
        .unwrap_or_default();

    // Also use UserSyncCookie for TTL-aware UID validity checks
    let cookie = UserSyncCookie::from_request(&headers, &state.host_cookie.cookie_name);
    let has_cookie = !cookie.uids.is_empty();

    let gdpr = body.gdpr;
    let gdpr_consent = body.gdpr_consent.clone();
    let coop_sync = body.coop_sync.unwrap_or(false);

    // ── Compute effective limit (account defaults + max limit) ───────────────
    let account_default_limit = account_cfg
        .and_then(|a| a.cookie_sync.default_limit);
    let account_max_limit = account_cfg
        .and_then(|a| a.cookie_sync.max_limit);
    let limit = compute_effective_limit(body.limit, account_default_limit, account_max_limit);

    // ── Priority groups from account or global config ────────────────────────
    let priority_groups: Vec<Vec<String>> = account_cfg
        .map(|a| a.cookie_sync.priority_groups.clone())
        .unwrap_or_default();

    // Determine which bidders to check — use requested list or fall back to all known
    let requested: Vec<String> = body.bidders.unwrap_or_default();

    // Cap returned *new* syncs at `limit`; already-synced bidders do NOT count against it.
    let limit = body.limit.unwrap_or(10).max(1) as usize;

    let mut bidder_status: Vec<serde_json::Value> = Vec::new();
    let mut new_sync_count: usize = 0;

    // Build the candidate list: bidders with sync info loaded from YAML take priority,
    // supplemented by the static KNOWN_SYNC_BIDDERS list.
    let all_known: std::collections::HashSet<&str> = KNOWN_SYNC_BIDDERS.iter().copied().collect();

    // Helper: build the full set of all known bidders (from YAML + static list).
    let build_all_bidders = || -> Vec<String> {
        let mut all: Vec<String> = state.bidder_sync_info.keys().cloned().collect();
        for b in KNOWN_SYNC_BIDDERS {
            if !state.bidder_sync_info.contains_key(*b) {
                all.push(b.to_string());
            }
        }
        all
    };

    let candidates: Vec<String> = if requested.is_empty() {
        // When no bidders requested: if coop_sync is true, include all known bidders.
        // Otherwise also include all known (same behavior, but coop_sync is the
        // explicit opt-in for cooperative syncing).
        build_all_bidders()
    } else if coop_sync {
        // Cooperative sync: start with the requested bidders, then append all
        // remaining known bidders that were not explicitly requested.
        let mut combined = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for name in &requested {
            let key = name.as_str();
            if state.bidder_sync_info.contains_key(key)
                || all_known.contains(key)
                || state.bidder_info.contains_key(key)
            {
                combined.push(name.clone());
                seen.insert(name.clone());
            } else {
                tracing::warn!("cookie_sync: unknown bidder '{}' requested; skipping", name);
            }
        }
        // Append cooperative bidders not already in the list.
        for b in build_all_bidders() {
            if !seen.contains(&b) {
                combined.push(b);
            }
        }
        combined
    } else {
        let mut valid = Vec::new();
        for name in &requested {
            let key = name.as_str();
            if state.bidder_sync_info.contains_key(key)
                || all_known.contains(key)
                || state.bidder_info.contains_key(key)
            {
                valid.push(name.clone());
            } else {
                tracing::warn!("cookie_sync: unknown bidder '{}' requested; skipping", name);
            }
        }
        valid
    };

    for bidder in &candidates {
        // ── Activity control: skip bidders where SyncUser is denied ───────
        let component = ActivityComponent {
            component_type: "bidder".to_string(),
            component_name: bidder.clone(),
        };
        if !state.activity_control.is_allowed(Activity::SyncUser, &component) {
            tracing::debug!("cookie_sync: SyncUser activity denied for bidder '{}'", bidder);
            continue;
        }

        // Already synced — check both TTL-aware cookie and PrebidCookie
        let has_uid_via_prebid = prebid_cookie.get_uid(bidder).is_some();
        if cookie.has_valid_uid(bidder) || has_uid_via_prebid {
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

        // Prefer sync info loaded from YAML, fall back to static map.
        let resolved = if let Some(sync_info) = state.bidder_sync_info.get(bidder.as_str()) {
            // Determine type: prefer redirect over iframe if both present
            if let Some(url) = sync_info.redirect_url.as_deref() {
                Some(("redirect", url.to_string()))
            } else if let Some(url) = sync_info.iframe_url.as_deref() {
                Some(("iframe", url.to_string()))
            } else {
                None
            }
        } else {
            bidder_sync_url(bidder).map(|(t, u)| (t, u.to_string()))
        };

        if let Some((sync_type_str, url_template)) = resolved {
            let sync_type = if sync_type_str == "iframe" { SyncType::Iframe } else { SyncType::Redirect };
            let syncer = Syncer {
                bidder: bidder.clone(),
                iframe_url: if sync_type_str == "iframe" { Some(url_template.clone()) } else { None },
                redirect_url: if sync_type_str != "iframe" { Some(url_template.clone()) } else { None },
            };
            let gdpr_val = gdpr.unwrap_or(0);
            let consent_val = gdpr_consent.as_deref().unwrap_or("");
            let url = syncer
                .get_sync_url(&sync_type, gdpr_val, consent_val)
                .unwrap_or_else(|| {
                    build_sync_url(&url_template, gdpr, gdpr_consent.as_deref())
                });
            bidder_status.push(serde_json::json!({
                "bidder": bidder,
                "no_cookie": true,
                "usersync": {
                    "url": url,
                    "type": sync_type_str
                }
            }));
            new_sync_count += 1;
        }
    }

    let status = if has_cookie { "ok" } else { "no_cookie" };

    Json(CookieSyncResponse {
        status: status.to_string(),
        bidder_status,
        debug: None,
    })
    .into_response()
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
    State(state): State<AppState>,
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

    // Record and log the event
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
            state.metrics.record_request_by_type("event_win", pbs_metrics::RequestStatus::Ok);
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
            state.metrics.record_request_by_type("event_imp", pbs_metrics::RequestStatus::Ok);
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
// GET /currency/rates — current currency conversion rates
// ──────────────────────────────────────────────────────────────────────────────

/// Response shape for the `/currency/rates` endpoint, mirroring Go's
/// `currencyRatesInfo` struct.
#[derive(Serialize)]
pub struct CurrencyRatesInfo {
    pub active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(rename = "lastUpdated", skip_serializing_if = "Option::is_none")]
    pub last_updated: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rates: Option<HashMap<String, HashMap<String, f64>>>,
}

pub async fn currency_rates_handler(State(state): State<AppState>) -> Json<CurrencyRatesInfo> {
    match &state.currency_converter {
        Some(converter) => {
            Json(CurrencyRatesInfo {
                active: true,
                source: converter.source().map(|s| s.to_string()),
                last_updated: converter.last_updated(),
                rates: Some(converter.rates().clone()),
            })
        }
        None => {
            Json(CurrencyRatesInfo {
                active: false,
                source: None,
                last_updated: None,
                rates: None,
            })
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Interstitial processing (applied before auction for instl=1 impressions)
// ──────────────────────────────────────────────────────────────────────────────

/// Common interstitial ad sizes, sorted by frequency / size (larger first).
/// Mirrors Go `config.ResolvedInterstitialSizes`.
const INTERSTITIAL_SIZES: &[(i32, i32)] = &[
    (300, 250), (728, 90), (160, 600), (320, 50), (300, 600),
    (320, 480), (336, 280), (120, 600), (468, 60), (970, 90),
    (970, 250), (320, 100), (300, 50), (300, 100), (768, 1024),
    (1024, 768), (480, 320), (300, 1050), (320, 250), (250, 250),
];

/// Process interstitial impressions by expanding banner format lists to include
/// standard sizes that fit between the device dimensions and the minimum
/// percentage thresholds from `device.ext.prebid.interstitial`.
///
/// This is a pre-auction transformation matching Go's `processInterstitials`.
pub fn process_interstitials(bid_request: &mut openrtb::BidRequest) {
    // Read min width/height percentage from device.ext.prebid.interstitial
    let interstitial = bid_request
        .device
        .as_ref()
        .and_then(|d| d.ext.as_ref())
        .and_then(|e| e.get("prebid"))
        .and_then(|p| p.get("interstitial"));

    let interstitial = match interstitial {
        Some(v) => v.clone(),
        None => return, // no interstitial config, nothing to do
    };

    let min_width_perc = interstitial
        .get("minwidthperc")
        .and_then(|v| v.as_i64())
        .unwrap_or(0) as i32;
    let min_height_perc = interstitial
        .get("minheightperc")
        .and_then(|v| v.as_i64())
        .unwrap_or(0) as i32;

    if min_width_perc <= 0 || min_height_perc <= 0 {
        return;
    }

    let device_w = bid_request
        .device
        .as_ref()
        .and_then(|d| d.w)
        .unwrap_or(0);
    let device_h = bid_request
        .device
        .as_ref()
        .and_then(|d| d.h)
        .unwrap_or(0);

    for imp in &mut bid_request.imp {
        if imp.instl != Some(1) {
            continue;
        }
        let banner = match &mut imp.banner {
            Some(b) => b,
            None => continue,
        };

        // Determine max dimensions from first format entry or device size.
        let (max_w, max_h) = if let Some(first) = banner.format.as_ref().and_then(|f| f.first()) {
            let fw = first.w.unwrap_or(0);
            let fh = first.h.unwrap_or(0);
            if fw < 2 && fh < 2 { (device_w, device_h) } else { (fw, fh) }
        } else {
            (device_w, device_h)
        };

        if max_w <= 0 || max_h <= 0 {
            continue;
        }

        let min_w = (max_w * min_width_perc) / 100;
        let min_h = (max_h * min_height_perc) / 100;

        let mut formats: Vec<openrtb::Format> = Vec::new();
        for &(w, h) in INTERSTITIAL_SIZES {
            if w >= min_w && w <= max_w && h >= min_h && h <= max_h {
                formats.push(openrtb::Format {
                    w: Some(w),
                    h: Some(h),
                    ..Default::default()
                });
                if formats.len() >= 10 {
                    break;
                }
            }
        }
        if !formats.is_empty() {
            banner.format = Some(formats);
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// GET/POST /optout — opt-out / opt-in handler
// ──────────────────────────────────────────────────────────────────────────────

/// Query parameters accepted by the `/optout` endpoint.
#[derive(Debug, Deserialize, Default)]
pub struct OptOutParams {
    /// When non-empty the user is opting **out**; when empty the user is opting
    /// back **in**.  Mirrors the Go `r.FormValue("optout")`.
    #[serde(default)]
    pub optout: Option<String>,
}

/// Handles GET and POST `/optout`.
///
/// Behaviour mirrors the Go `UserSyncDeps.OptOut` handler:
///  1. Read the prebid UID cookie from the request.
///  2. Set or clear the `optout` flag based on the `optout` query parameter.
///  3. Write the updated cookie back via `Set-Cookie`.
///  4. Redirect to `opt_out_url` (if opting out) or `opt_in_url` (if opting in).
///
/// The Go handler additionally checks a reCAPTCHA response and, when it is
/// absent, redirects to a static HTML page.  The Rust port omits the reCAPTCHA
/// verification but preserves the rest of the flow so that automated / API
/// callers can toggle opt-out programmatically.
pub async fn optout_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<OptOutParams>,
) -> Response {
    let hc = &state.host_cookie;

    // Read the current prebid UID cookie.
    let mut cookie = UserSyncCookie::from_request(&headers, &hc.cookie_name);

    // Determine whether this is an opt-out or opt-in request.
    let is_opt_out = params.optout.as_ref().map(|v| !v.is_empty()).unwrap_or(false);
    cookie.optout = Some(is_opt_out);

    // If opting out, clear all stored UIDs so no further syncing occurs.
    if is_opt_out {
        cookie.uids.clear();
    }

    // Build the Set-Cookie header with the updated cookie.
    let set_cookie_value = cookie.build_set_cookie_header(
        &hc.cookie_name,
        hc.ttl_days,
        &hc.domain,
    );

    let redirect_url = if is_opt_out {
        &hc.opt_out_url
    } else {
        &hc.opt_in_url
    };

    // If no redirect URL is configured, respond with a simple 200 OK.
    if redirect_url.is_empty() {
        let mut resp = (StatusCode::OK, "opt-out preference saved").into_response();
        if let Ok(val) = HeaderValue::from_str(&set_cookie_value) {
            resp.headers_mut().insert(SET_COOKIE, val);
        }
        return resp;
    }

    // Redirect (301 Moved Permanently) to the configured URL, matching Go behaviour.
    let mut resp = axum::response::Redirect::permanent(redirect_url).into_response();
    if let Ok(val) = HeaderValue::from_str(&set_cookie_value) {
        resp.headers_mut().insert(SET_COOKIE, val);
    }
    resp
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
        .route("/openrtb2/auction", axum::routing::get(auction_get_handler).post(auction_handler))
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
        .route("/optout", axum::routing::get(optout_handler).post(optout_handler))
        // Events
        .route("/event", axum::routing::get(event_handler))
        .route("/vtrack", axum::routing::post(vtrack_handler))
        // Currency
        .route("/currency/rates", axum::routing::get(currency_rates_handler))
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
            bidder_sync_info: std::collections::HashMap::new(),
            currency_converter: None,
            account_required: false,
            activity_control: ActivityControl::default(),
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
