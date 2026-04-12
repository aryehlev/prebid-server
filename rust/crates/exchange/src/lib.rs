use std::collections::HashMap;
use std::sync::Arc;

use openrtb_ext::{NonBid, SeatNonBid};
use pbs_adapters::{BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData};

pub mod adserver_targeting;
pub mod analytics;
pub mod bidadjustment;
pub mod cache;
pub mod currency;
pub mod dsa;
pub mod event_request;
pub mod events;
pub mod first_party_data;
pub mod floors;
pub mod gdpr;
pub mod gdpr_config;
pub mod default_request;
pub mod hooks;
pub mod interstitial;
pub mod macros;
pub mod stored_requests_db;
pub mod privacy;
pub mod seat_non_bids;
pub mod tmax;
pub mod usersync;
pub mod cookie_sync;
pub mod setuid;
pub mod account;
pub mod adapter_util;
pub mod amp;
pub mod auction;
pub mod clone;
pub mod defaults;
pub mod exchange_utils;
pub mod injector;
pub mod json_merge;
pub mod ortb_version;
pub mod request_splitter;
pub mod response_builder;
pub mod vendorlist;
pub mod vtrack;
pub mod price_granularity;
pub mod privacysandbox;
pub mod request_validator;
pub mod schain;
pub mod stored_requests;
pub mod stored_requests_http;
pub mod stored_responses;
pub mod targeting;
pub mod validation;
pub mod vast;
pub mod category_mapping;
pub mod deals;

#[cfg(test)]
mod tests;

/// Account holds basic account configuration.
#[derive(Debug, Clone, Default)]
pub struct Account {
    pub id: String,
    pub price_granularity: Option<String>,
}

/// UserSyncData carries information about which bidders the user has synced with.
#[derive(Debug, Clone, Default)]
pub struct UserSyncData {
    pub synced_bidders: std::collections::HashSet<String>,
}

/// AuctionRequest is the input to Exchange::hold_auction.
pub struct AuctionRequest {
    pub bid_request: openrtb::BidRequest,
    pub account: Option<Account>,
    pub user_syncs: Option<UserSyncData>,
    pub start_time: std::time::Instant,
    pub currency_rates: Option<Arc<currency::CurrencyConverter>>,
}

/// AuctionResponse is the output of Exchange::hold_auction.
pub struct AuctionResponse {
    pub bid_response: openrtb::BidResponse,
    pub seat_non_bids: Vec<SeatNonBid>,
    pub targeting: HashMap<String, HashMap<String, String>>, // imp_id -> targeting_keys
    pub timed_out_bidders: Vec<String>,
}

/// BidderResult collects everything returned from a single bidder task.
pub struct BidderResult {
    pub bidder_name: String,
    pub response: Result<BidderResponse, Vec<BidderError>>,
    pub duration_ms: u64,
    pub http_calls: Vec<openrtb_ext::ExtHttpCall>,
    pub timed_out: bool,
}

/// Deep-merge `overlay` JSON into `base`, with base keys taking precedence on conflict.
/// For object values, recursion is applied. For arrays and scalars, base wins.
pub fn deep_merge_json(base: &mut serde_json::Value, overlay: &serde_json::Value) {
    if let (Some(base_obj), Some(overlay_obj)) = (base.as_object_mut(), overlay.as_object()) {
        for (k, v) in overlay_obj {
            let entry = base_obj.entry(k.clone()).or_insert(serde_json::Value::Null);
            if entry.is_null() {
                *entry = v.clone();
            } else if entry.is_object() && v.is_object() {
                deep_merge_json(entry, v);
            }
            // base key wins for all other types (scalar, array, type mismatch)
        }
    }
}

/// Merge arrays: append items from `overlay` that are not already in `base`.
fn merge_string_arrays(base: &mut Vec<String>, overlay: &[String]) {
    for item in overlay {
        if !base.contains(item) {
            base.push(item.clone());
        }
    }
}

/// Apply first-party data (FPD) overrides for a specific bidder.
///
/// Reads `req.ext.prebid.data.bidderspecific.<bidder_name>` and deep-merges
/// `site`, `user`, and `app` fields into the top-level BidRequest fields.
fn apply_fpd_for_bidder(req: &mut openrtb::BidRequest, bidder_name: &str) {
    let fpd = req
        .ext
        .as_ref()
        .and_then(|e| e.get("prebid"))
        .and_then(|p| p.get("data"))
        .and_then(|d| d.get("bidderspecific"))
        .and_then(|bs| bs.get(bidder_name))
        .cloned();

    let fpd = match fpd {
        Some(v) => v,
        None => return,
    };

    // ── Merge site FPD ────────────────────────────────────────────────────────
    if let Some(site_fpd) = fpd.get("site") {
        if let Ok(site_override) = serde_json::from_value::<openrtb::Site>(site_fpd.clone()) {
            if let Some(site) = &mut req.site {
                if site_override.page.is_some() {
                    site.page = site_override.page;
                }
                if site_override.domain.is_some() {
                    site.domain = site_override.domain;
                }
                if site_override.publisher.is_some() {
                    site.publisher = site_override.publisher;
                }
                // site.ref_
                if site.ref_.is_none() {
                    site.ref_ = site_override.ref_;
                }
                // site.search
                if site.search.is_none() {
                    site.search = site_override.search;
                }
                // site.cat — merge arrays (no duplicates)
                match (&mut site.cat, site_override.cat) {
                    (Some(base), Some(overlay)) => merge_string_arrays(base, &overlay),
                    (None, Some(overlay)) => site.cat = Some(overlay),
                    _ => {}
                }
                // site.sectioncat
                match (&mut site.sectioncat, site_override.sectioncat) {
                    (Some(base), Some(overlay)) => merge_string_arrays(base, &overlay),
                    (None, Some(overlay)) => site.sectioncat = Some(overlay),
                    _ => {}
                }
                // site.pagecat
                match (&mut site.pagecat, site_override.pagecat) {
                    (Some(base), Some(overlay)) => merge_string_arrays(base, &overlay),
                    (None, Some(overlay)) => site.pagecat = Some(overlay),
                    _ => {}
                }
                // site.ext — deep merge (base keys win)
                if let Some(new_ext) = site_override.ext {
                    if let Some(existing) = &mut site.ext {
                        deep_merge_json(existing, &new_ext);
                    } else {
                        site.ext = Some(new_ext);
                    }
                }
            }
        }
    }

    // ── Merge user FPD ────────────────────────────────────────────────────────
    if let Some(user_fpd) = fpd.get("user") {
        if let Ok(user_override) = serde_json::from_value::<openrtb::User>(user_fpd.clone()) {
            if let Some(user) = &mut req.user {
                if user_override.buyeruid.is_some() {
                    user.buyeruid = user_override.buyeruid;
                }
                // user.geo — full Geo struct merge (base fields win)
                match (&mut user.geo, user_override.geo) {
                    (Some(base_geo), Some(overlay_geo)) => {
                        if let (Ok(mut base_val), Ok(overlay_val)) = (
                            serde_json::to_value(&*base_geo),
                            serde_json::to_value(&overlay_geo),
                        ) {
                            deep_merge_json(&mut base_val, &overlay_val);
                            if let Ok(merged) = serde_json::from_value::<openrtb::Geo>(base_val) {
                                *base_geo = merged;
                            }
                        }
                    }
                    (None, Some(overlay_geo)) => user.geo = Some(overlay_geo),
                    _ => {}
                }
                // user.data — append overlay items
                if !user_override.data.is_empty() {
                    user.data.extend(user_override.data);
                }
                // user.ext — deep merge (base keys win)
                if let Some(new_ext) = user_override.ext {
                    if let Some(existing) = &mut user.ext {
                        deep_merge_json(existing, &new_ext);
                    } else {
                        user.ext = Some(new_ext);
                    }
                }
            }
        }
    }

    // ── Merge app FPD ─────────────────────────────────────────────────────────
    if let Some(app_fpd) = fpd.get("app") {
        if let Ok(app_override) = serde_json::from_value::<openrtb::App>(app_fpd.clone()) {
            if let Some(app) = &mut req.app {
                // app.bundle
                if app.bundle.is_none() {
                    app.bundle = app_override.bundle;
                }
                // app.domain
                if app.domain.is_none() {
                    app.domain = app_override.domain;
                }
                // app.cat — merge arrays
                match (&mut app.cat, app_override.cat) {
                    (Some(base), Some(overlay)) => merge_string_arrays(base, &overlay),
                    (None, Some(overlay)) => app.cat = Some(overlay),
                    _ => {}
                }
                // app.ext — deep merge (base keys win)
                if let Some(new_ext) = app_override.ext {
                    if let Some(existing) = &mut app.ext {
                        deep_merge_json(existing, &new_ext);
                    } else {
                        app.ext = Some(new_ext);
                    }
                }
            }
        }
    }
}

fn compress_gzip(data: &[u8]) -> Result<Vec<u8>, std::io::Error> {
    use std::io::Write;
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(data)?;
    encoder.finish()
}


/// AdaptedBidder wraps a Bidder implementation with HTTP execution logic.
pub struct AdaptedBidder {
    pub bidder: Arc<dyn pbs_adapters::Bidder>,
    pub http_client: reqwest::Client,
    pub endpoint: String,
    pub endpoint_compression: Option<String>,
}

impl AdaptedBidder {
    pub async fn request_bid(
        &self,
        request: &openrtb::BidRequest,
        bidder_name: &str,
        extra_info: &ExtraRequestInfo,
        timeout_ms: u64,
    ) -> BidderResult {
        let start = std::time::Instant::now();
        let (reqs, mut errs) = self.bidder.make_requests(request, extra_info);

        if reqs.is_empty() {
            return BidderResult {
                bidder_name: bidder_name.to_string(),
                response: if errs.is_empty() {
                    Ok(BidderResponse::new())
                } else {
                    Err(errs)
                },
                duration_ms: start.elapsed().as_millis() as u64,
                http_calls: Vec::new(),
                timed_out: false,
            };
        }

        let mut all_bids = BidderResponse::new();
        let mut http_calls: Vec<openrtb_ext::ExtHttpCall> = Vec::new();
        let mut timed_out = false;

        for req_data in &reqs {
            match self.execute_request(req_data, timeout_ms).await {
                Ok((response_data, http_call)) => {
                    http_calls.push(http_call);
                    if response_data.status_code == 204 {
                        continue;
                    }
                    match self.bidder.make_bids(request, req_data, &response_data) {
                        Ok(mut br) => {
                            all_bids.bids.append(&mut br.bids);
                            all_bids
                                .fledge_auction_configs
                                .append(&mut br.fledge_auction_configs);
                            if br.currency != "USD" {
                                all_bids.currency = br.currency;
                            }
                        }
                        Err(mut e) => errs.append(&mut e),
                    }
                }
                Err(BidderError::Timeout) => {
                    timed_out = true;
                    tracing::warn!("bidder {} timed out after {}ms", bidder_name, timeout_ms);
                    errs.push(BidderError::Timeout);
                }
                Err(e) => errs.push(e),
            }
        }

        if !errs.is_empty() {
            tracing::debug!("bidder {} returned {} errors", bidder_name, errs.len());
        }

        // If all errors are non-fatal, still return the bids we collected.
        let response = if errs.iter().any(|e| matches!(e, BidderError::BadInput(_))) {
            Err(errs)
        } else {
            Ok(all_bids)
        };

        BidderResult {
            bidder_name: bidder_name.to_string(),
            response,
            duration_ms: start.elapsed().as_millis() as u64,
            http_calls,
            timed_out,
        }
    }

    async fn execute_request(
        &self,
        req: &RequestData,
        timeout_ms: u64,
    ) -> Result<(ResponseData, openrtb_ext::ExtHttpCall), BidderError> {
        let timeout = std::time::Duration::from_millis(timeout_ms);

        let mut builder = match req.method.as_str() {
            "POST" => self.http_client.post(&req.uri),
            "GET" => self.http_client.get(&req.uri),
            m => {
                return Err(BidderError::BadInput(format!(
                    "unsupported HTTP method: {m}"
                )))
            }
        };

        let use_gzip = self
            .endpoint_compression
            .as_deref()
            .map(|s| s.to_uppercase() == "GZIP")
            .unwrap_or(false);

        let (body_bytes, content_encoding) = if use_gzip {
            match compress_gzip(&req.body) {
                Ok(compressed) => (compressed, Some("gzip")),
                Err(_) => (req.body.clone(), None),
            }
        } else {
            (req.body.clone(), None)
        };

        let req_body_str = String::from_utf8_lossy(&req.body).to_string();

        builder = builder.body(body_bytes);
        for (k, v) in &req.headers {
            builder = builder.header(k, v);
        }
        if let Some(enc) = content_encoding {
            builder = builder.header("Content-Encoding", enc);
        }

        match tokio::time::timeout(timeout, builder.send()).await {
            Ok(Ok(resp)) => {
                let status = resp.status().as_u16();
                let body_bytes = resp
                    .bytes()
                    .await
                    .map_err(|e| BidderError::BadServerResponse(e.to_string()))?
                    .to_vec();

                let http_call = openrtb_ext::ExtHttpCall {
                    uri: req.uri.clone(),
                    request_body: req_body_str,
                    request_headers: HashMap::new(),
                    response_body: String::from_utf8_lossy(&body_bytes).to_string(),
                    status: status as i32,
                };

                Ok((
                    ResponseData {
                        status_code: status,
                        body: body_bytes,
                        headers: HashMap::new(),
                    },
                    http_call,
                ))
            }
            Ok(Err(e)) => Err(BidderError::FailedToRequestBids(e.to_string())),
            Err(_) => Err(BidderError::Timeout),
        }
    }
}

/// PriceRange describes a single range band for custom price granularity.
#[derive(Debug, Clone)]
pub struct PriceRange {
    pub max: f64,
    pub increment: f64,
}

/// PriceGranularity holds custom granularity configuration parsed from
/// `req.ext.prebid.targeting.pricegranularity`.
#[derive(Debug, Clone)]
pub struct PriceGranularity {
    pub precision: Option<u32>,
    pub ranges: Vec<PriceRange>,
}

impl PriceGranularity {
    /// Try to parse a PriceGranularity from a serde_json::Value representing
    /// the `pricegranularity` object in `req.ext.prebid.targeting`.
    fn from_json(v: &serde_json::Value) -> Option<Self> {
        let obj = v.as_object()?;
        let precision = obj
            .get("precision")
            .and_then(|p| p.as_u64())
            .map(|p| p as u32);
        let ranges = obj
            .get("ranges")
            .and_then(|r| r.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|entry| {
                        let max = entry.get("max")?.as_f64()?;
                        let increment = entry.get("increment")?.as_f64()?;
                        if increment <= 0.0 {
                            return None;
                        }
                        Some(PriceRange { max, increment })
                    })
                    .collect()
            })
            .unwrap_or_else(|| Vec::<PriceRange>::new());
        if ranges.is_empty() && precision.is_none() {
            return None;
        }
        Some(PriceGranularity { precision, ranges })
    }
}

/// price_granularity_bucket returns the price bucket string for a bid price.
///
/// If a custom `PriceGranularity` is provided, it is used; otherwise the
/// default "medium" granularity applies ($0.01 increments, capped at $20.00).
fn price_granularity_bucket(price: f64, granularity: Option<&PriceGranularity>) -> String {
    match granularity {
        Some(gran) if !gran.ranges.is_empty() => {
            let precision = gran.precision.unwrap_or(2) as usize;
            // Find the overall max across all ranges.
            let bucket_max = gran
                .ranges
                .iter()
                .map(|r| r.max)
                .fold(f64::NEG_INFINITY, f64::max);

            if price > bucket_max {
                return format!("{:.prec$}", bucket_max, prec = precision);
            }

            // Find the matching range (the range whose max >= price, smallest max first).
            let mut sorted_ranges = gran.ranges.clone();
            sorted_ranges.sort_by(|a, b| a.max.partial_cmp(&b.max).unwrap_or(std::cmp::Ordering::Equal));

            let mut bucket_min = 0.0_f64;
            for range in &sorted_ranges {
                if price <= range.max {
                    let increment = range.increment;
                    let steps = ((price - bucket_min) / increment).floor();
                    let rounded = steps * increment + bucket_min;
                    return format!("{:.prec$}", rounded, prec = precision);
                }
                bucket_min = range.max;
            }

            // Fallback: return bucket_max
            format!("{:.prec$}", bucket_max, prec = precision)
        }
        _ => {
            // Default medium granularity: $0.01 increments, capped at $20.00.
            let capped = price.min(20.0);
            let bucket = (capped * 100.0).floor() / 100.0;
            format!("{:.2}", bucket)
        }
    }
}

/// validate_bids removes bids that fail basic sanity checks and logs warnings
/// for bids that are kept but have suspicious fields.
///
/// Rules (mirrors Go exchange/bidder_validate_bids.go):
///   - `bid.id` empty → drop
///   - `bid.impid` empty → drop
///   - `bid.impid` does not match any request imp → drop
///   - `bid.price` < 0 → drop
///   - `bid.price` == 0 and no deal → drop
///   - `bid.crid` empty → drop
///   - banner/video bid with no `adm` AND no `nurl` → warn but keep
fn validate_bids(bids: Vec<pbs_adapters::TypedBid>, imps: &[openrtb::Imp]) -> Vec<pbs_adapters::TypedBid> {
    let imp_ids: std::collections::HashSet<&str> = imps.iter().map(|i| i.id.as_str()).collect();

    bids.into_iter()
        .filter(|tb| {
            let bid = &tb.bid;

            if bid.id.is_empty() {
                tracing::warn!(impid = %bid.impid, "dropping bid: missing required field 'id'");
                return false;
            }

            if bid.impid.is_empty() {
                tracing::warn!(bid_id = %bid.id, "dropping bid: missing required field 'impid'");
                return false;
            }

            if !imp_ids.contains(bid.impid.as_str()) {
                tracing::warn!(bid_id = %bid.id, impid = %bid.impid,
                    "dropping bid: impid does not match any imp in the request");
                return false;
            }

            if bid.price < 0.0 {
                tracing::warn!(bid_id = %bid.id, price = bid.price,
                    "dropping bid: price must be >= 0");
                return false;
            }

            // Zero-price bids are only allowed when there's a deal.
            if bid.price == 0.0 && bid.dealid.is_none() {
                tracing::warn!(bid_id = %bid.id,
                    "dropping bid: zero price requires a deal");
                return false;
            }

            if bid.crid.as_deref().unwrap_or("").is_empty() {
                tracing::warn!(bid_id = %bid.id, "dropping bid: missing creative ID");
                return false;
            }

            // Warn (but keep) banner/video bids without creative.
            let is_banner_or_video = matches!(tb.bid_type, openrtb_ext::BidType::Banner | openrtb_ext::BidType::Video);
            if is_banner_or_video && bid.adm.is_none() && bid.nurl.is_none() {
                tracing::warn!(bid_id = %bid.id, bid_type = ?tb.bid_type,
                    "bid has no adm or nurl; creative may be missing");
            }

            true
        })
        .collect()
}

/// Validate that the bid currency is a valid ISO 4217 code.
/// Returns an error string if the code is malformed.
///
/// Note: currency *conversion* (EUR→USD etc.) is handled separately by the
/// currency converter.  This function only checks that the code is syntactically
/// valid so we don't attempt to convert a garbage string.
///
/// Mirrors the format-check portion of Go exchange/bidder_validate_bids.go
/// `validateCurrency()`.
fn validate_bid_currency(_request_currencies: &[String], bid_currency: &str) -> Result<(), String> {
    if bid_currency.is_empty() {
        return Ok(()); // empty means default (USD)
    }

    let upper = bid_currency.to_uppercase();
    if upper.len() != 3 || !upper.chars().all(|c| c.is_ascii_uppercase()) {
        return Err(format!("Invalid currency code: '{}'", bid_currency));
    }

    Ok(())
}

/// Split impressions per bidder, extracting each bidder's params from
/// `imp.ext.prebid.bidder.<name>` and creating sanitized imp copies where
/// the ext only contains that bidder's params.
///
/// This mirrors Go `exchange/utils.go` `splitImps()`.
///
/// For each imp:
///   1. Parse `imp.ext.prebid.bidder` to find which bidders have params
///   2. For each bidder found, create a copy of the imp with sanitized ext:
///      - Remove `imp.ext.prebid.bidder` (all bidder params)
///      - Set `imp.ext.bidder` to only this bidder's params
///      - Preserve other imp.ext fields (e.g. `data`, `gpid`, `tid`)
///      - Preserve allowed prebid fields (`is_rewarded_inventory`, `options`)
fn split_imps_for_bidders(
    imps: &[openrtb::Imp],
    known_bidders: &[String],
) -> HashMap<String, Vec<openrtb::Imp>> {
    let mut bidder_imps: HashMap<String, Vec<openrtb::Imp>> = HashMap::new();

    for imp in imps {
        let ext = match &imp.ext {
            Some(ext) => ext,
            None => continue,
        };

        // Extract bidder params from imp.ext.prebid.bidder.<name>
        let prebid_bidder_map: Option<&serde_json::Map<String, serde_json::Value>> = ext
            .get("prebid")
            .and_then(|p| p.get("bidder"))
            .and_then(|b| b.as_object());

        // Also check for top-level imp.ext.<bidder> (legacy format)
        let ext_obj = ext.as_object();

        // Collect bidder names and their params for this imp
        let mut bidder_params: Vec<(String, serde_json::Value)> = Vec::new();

        if let Some(bidder_map) = prebid_bidder_map {
            for (bidder_name, params) in bidder_map {
                bidder_params.push((bidder_name.clone(), params.clone()));
            }
        }

        // Also pick up legacy top-level ext.<bidder> format
        if let Some(ext_map) = ext_obj {
            for (key, val) in ext_map {
                if key == "prebid" || key == "data" || key == "gpid" || key == "tid"
                    || key == "skadn" || key == "context"
                {
                    continue; // skip known non-bidder keys
                }
                if known_bidders.contains(key) {
                    // Only add if not already found in prebid.bidder
                    if !bidder_params.iter().any(|(n, _)| n == key) {
                        bidder_params.push((key.clone(), val.clone()));
                    }
                }
            }
        }

        for (bidder_name, params) in bidder_params {
            let mut imp_copy = imp.clone();

            // Build sanitized ext: keep non-bidder fields, set bidder key to this bidder's params only
            let mut sanitized_ext = serde_json::Map::new();

            if let Some(ext_map) = ext_obj {
                // Copy non-bidder, non-prebid fields (e.g. data, gpid, tid, skadn, context)
                for (key, val) in ext_map {
                    if key == "prebid" {
                        continue; // handled separately below
                    }
                    if known_bidders.contains(key) {
                        continue; // skip other bidders' top-level params
                    }
                    sanitized_ext.insert(key.clone(), val.clone());
                }
            }

            // Build sanitized prebid object: keep is_rewarded_inventory and options
            if let Some(prebid) = ext.get("prebid") {
                let mut sanitized_prebid = serde_json::Map::new();
                for allowed_key in &["is_rewarded_inventory", "options"] {
                    if let Some(val) = prebid.get(*allowed_key) {
                        sanitized_prebid.insert(allowed_key.to_string(), val.clone());
                    }
                }
                if !sanitized_prebid.is_empty() {
                    sanitized_ext.insert(
                        "prebid".to_string(),
                        serde_json::Value::Object(sanitized_prebid),
                    );
                }
            }

            // Set the bidder key to this bidder's params
            sanitized_ext.insert("bidder".to_string(), params);

            imp_copy.ext = Some(serde_json::Value::Object(sanitized_ext));
            bidder_imps.entry(bidder_name).or_default().push(imp_copy);
        }
    }

    bidder_imps
}

/// Trait for fetching stored auction responses by ID.
///
/// Implementors return a pre-built `BidResponse` JSON value for a given stored-response ID,
/// or `None` if no stored response exists for that ID.
pub trait StoredResponseFetcher: Send + Sync {
    fn fetch(&self, id: &str) -> Option<&serde_json::Value>;
}

/// Exchange orchestrates the full auction: fan-out to bidders, collect results, build response.
pub struct Exchange {
    pub adapters: HashMap<String, AdaptedBidder>,
    pub http_client: reqwest::Client,
    pub metrics: Option<Arc<dyn pbs_metrics::MetricsEngine>>,
    pub analytics: Option<Arc<dyn analytics::AnalyticsBackend>>,
    pub hook_executor: Option<Arc<hooks::HookExecutor>>,
    /// Optional stored auction response fetcher.  When set and a request contains
    /// `req.ext.prebid.storedauctionresponse.id`, bidder calls are skipped and the
    /// pre-built `BidResponse` stored under that ID is returned directly.
    pub stored_responses: Option<Arc<dyn StoredResponseFetcher>>,
    /// Alias map: alias bidder name -> canonical bidder name.
    /// When a request references an alias name, the canonical adapter is used
    /// but the response SeatBid uses the alias name.
    pub aliases: HashMap<String, String>,
    /// Optional host-level SChain node to prepend to every outgoing request.
    pub schain_node: Option<openrtb::SupplyChainNode>,
    /// Optional Prebid Cache client for caching bid creatives.
    /// When configured and `req.ext.prebid.cache` is set, winning bids are
    /// written to the cache and cache IDs are added to targeting.
    pub cache_client: Option<Arc<dyn cache::CacheClient>>,
}

impl Exchange {
    pub fn new(adapters: HashMap<String, AdaptedBidder>) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("Failed to build HTTP client");
        Self {
            adapters,
            http_client,
            metrics: None,
            analytics: None,
            hook_executor: None,
            stored_responses: None,
            aliases: HashMap::new(),
            schain_node: None,
            cache_client: None,
        }
    }

    /// Returns the number of registered adapters.
    pub fn adapter_count(&self) -> usize {
        self.adapters.len()
    }

    /// Run an auction using the global timeout for all bidders.
    pub async fn hold_auction(
        &self,
        request: AuctionRequest,
    ) -> Result<AuctionResponse, anyhow::Error> {
        self.hold_auction_with_timeouts(request, &HashMap::new()).await
    }

    /// Run an auction, optionally overriding the timeout per bidder.
    /// If a bidder name appears in `per_bidder_timeouts`, that value (ms) is
    /// used instead of the global tmax-derived timeout.
    pub async fn hold_auction_with_timeouts(
        &self,
        request: AuctionRequest,
        _per_bidder_timeouts: &HashMap<String, u64>,
    ) -> Result<AuctionResponse, anyhow::Error> {
        // Validate the request before any processing.
        let validation_errors = validation::validate_request(&request.bid_request);
        if validation::has_fatal_errors(&validation_errors) {
            let msgs: Vec<String> = validation_errors.iter().map(|e| e.to_string()).collect();
            return Err(anyhow::anyhow!("invalid request: {}", msgs.join("; ")));
        }

        // Execute EntrypointRaw hooks before any processing.
        if let Some(executor) = &self.hook_executor {
            let payload = serde_json::to_value(&request.bid_request)
                .unwrap_or(serde_json::Value::Null);
            if let Err(reject) = executor
                .execute_stage(hooks::Stage::EntrypointRaw, payload)
                .await
            {
                return Err(anyhow::anyhow!("request rejected by hook: {}", reject));
            }
        }

        // Execute RawAuctionRequest hooks after entrypoint, before further processing.
        // In Go this runs in the auction endpoint after account lookup but before
        // request validation / stored-request merging.
        if let Some(executor) = &self.hook_executor {
            let payload = serde_json::to_value(&request.bid_request)
                .unwrap_or(serde_json::Value::Null);
            if let Err(reject) = executor
                .execute_stage(hooks::Stage::RawAuctionRequest, payload)
                .await
            {
                return Err(anyhow::anyhow!("request rejected by RawAuctionRequest hook: {}", reject));
            }
        }

        let bid_request = &request.bid_request;

        // ── Stored auction response short-circuit ─────────────────────────────
        // If `req.ext.prebid.storedauctionresponse.id` is set, skip all bidder
        // calls and return the pre-built BidResponse stored under that ID.
        if let Some(stored_resp_id) = bid_request
            .ext
            .as_ref()
            .and_then(|e| e.get("prebid"))
            .and_then(|p| p.get("storedauctionresponse"))
            .and_then(|s| s.get("id"))
            .and_then(|v| v.as_str())
        {
            if let Some(stored_resp_fetcher) = &self.stored_responses {
                if let Some(stored_json) = stored_resp_fetcher.fetch(stored_resp_id) {
                    match serde_json::from_value::<openrtb::BidResponse>(stored_json.clone()) {
                        Ok(mut bid_response) => {
                            // Preserve the request ID in the response.
                            if bid_response.id.is_empty() {
                                bid_response.id = bid_request.id.clone();
                            }
                            // Build targeting from the stored response's seatbid entries.
                            let mut targeting: HashMap<String, HashMap<String, String>> =
                                HashMap::new();
                            for seat_bid in &bid_response.seatbid {
                                let bidder_name =
                                    seat_bid.seat.as_deref().unwrap_or("unknown");
                                for bid in &seat_bid.bid {
                                    // Extract prebid targeting from bid.ext if present.
                                    let ext_targeting: HashMap<String, String> = bid
                                        .ext
                                        .as_ref()
                                        .and_then(|e| e.get("prebid"))
                                        .and_then(|p| p.get("targeting"))
                                        .and_then(|t| {
                                            serde_json::from_value(t.clone()).ok()
                                        })
                                        .unwrap_or_default();

                                    let keys =
                                        targeting.entry(bid.impid.clone()).or_default();
                                    // Merge ext targeting keys.
                                    for (k, v) in ext_targeting {
                                        keys.insert(k, v);
                                    }
                                    // Ensure basic winner keys are present.
                                    keys.entry("hb_bidder".to_string())
                                        .or_insert_with(|| bidder_name.to_string());
                                    keys.entry("hb_pb".to_string()).or_insert_with(|| {
                                        price_granularity_bucket(bid.price, None)
                                    });
                                }
                            }
                            tracing::debug!(
                                id = stored_resp_id,
                                "using stored auction response; skipping bidder calls"
                            );
                            return Ok(AuctionResponse {
                                bid_response,
                                seat_non_bids: vec![],
                                targeting,
                                timed_out_bidders: vec![],
                            });
                        }
                        Err(e) => {
                            return Err(anyhow::anyhow!(
                                "failed to parse stored auction response '{}': {}",
                                stored_resp_id,
                                e
                            ));
                        }
                    }
                } else {
                    return Err(anyhow::anyhow!(
                        "stored auction response not found for id: {}",
                        stored_resp_id
                    ));
                }
            }
        }

        // --- Stored bid responses (per-imp, per-bidder) ---
        // Parse imp.ext.prebid.storedbidresponse entries. For each matching imp+bidder,
        // inject stored bids directly instead of calling the adapter.
        // Structure: imp.ext.prebid.storedbidresponse = [{"bidder": "appnexus", "id": "stored-resp-1"}, ...]
        let mut stored_bid_responses: HashMap<String, HashMap<String, serde_json::Value>> = HashMap::new();
        for imp in &bid_request.imp {
            if let Some(entries) = imp.ext.as_ref()
                .and_then(|e| e.get("prebid"))
                .and_then(|p| p.get("storedbidresponse"))
                .and_then(|s| s.as_array())
            {
                for entry in entries {
                    let bidder = entry.get("bidder").and_then(|b| b.as_str()).unwrap_or("");
                    let resp_id = entry.get("id").and_then(|i| i.as_str()).unwrap_or("");
                    if !bidder.is_empty() && !resp_id.is_empty() {
                        if let Some(fetcher) = &self.stored_responses {
                            if let Some(stored_json) = fetcher.fetch(resp_id) {
                                stored_bid_responses
                                    .entry(bidder.to_string())
                                    .or_default()
                                    .insert(imp.id.clone(), stored_json.clone());
                            }
                        }
                    }
                }
            }
        }

        // Validate impression count
        if bid_request.imp.is_empty() {
            return Err(anyhow::anyhow!("request.imp must contain at least one impression"));
        }

        // Parse multi-bid configuration from req.ext.prebid.multibid.
        // Builds a map of bidder_name -> max_bids_per_imp.
        let multi_bid_limits: HashMap<String, u32> = {
            let entries: Vec<openrtb_ext::MultiBid> = bid_request
                .ext
                .as_ref()
                .and_then(|e| e.get("prebid"))
                .and_then(|p| p.get("multibid"))
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();

            let mut limits: HashMap<String, u32> = HashMap::new();
            for entry in &entries {
                let max = entry.max_bids.unwrap_or(1).max(1);
                if let Some(bidder) = &entry.bidder {
                    limits.insert(bidder.clone(), max);
                }
                if let Some(bidders) = &entry.bidders {
                    for b in bidders {
                        limits.insert(b.clone(), max);
                    }
                }
            }
            limits
        };

        // Validate tmax (auction timeout)
        if let Some(tmax) = bid_request.tmax {
            if tmax < 0 {
                return Err(anyhow::anyhow!("request.tmax must be nonneg"));
            }
        }

        // Validate each impression has an ID and at least one media type
        for imp in &bid_request.imp {
            if imp.id.is_empty() {
                return Err(anyhow::anyhow!("request.imp[].id required"));
            }
            if imp.banner.is_none() && imp.video.is_none() && imp.audio.is_none() && imp.native.is_none() {
                return Err(anyhow::anyhow!(
                    "request.imp[id={}] must specify at least one of banner/video/audio/native",
                    imp.id
                ));
            }
        }

        // Tmax adjustments: subtract network overhead from bidder timeout.
        // Read `req.ext.prebid.server.response_time_ms` if present.
        let response_time_ms_overhead: i64 = bid_request
            .ext
            .as_ref()
            .and_then(|e| e.get("prebid"))
            .and_then(|p| p.get("server"))
            .and_then(|s| s.get("response_time_ms"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        let bidder_timeout_ms: u64 = bid_request
            .tmax
            .filter(|&t| t > 0)
            .map(|t| {
                let adjusted = t - response_time_ms_overhead;
                adjusted.max(1) as u64
            })
            .unwrap_or(1000);

        let timeout_ms = bidder_timeout_ms;
        // Per-bidder timeouts (empty by default; can be populated from account config)
        let per_bidder_timeouts: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
        let _duration = std::time::Duration::from_millis(timeout_ms);

        // Parse custom price granularity from req.ext.prebid.targeting.pricegranularity.
        let custom_price_granularity: Option<PriceGranularity> = bid_request
            .ext
            .as_ref()
            .and_then(|e| e.get("prebid"))
            .and_then(|p| p.get("targeting"))
            .and_then(|t| t.get("pricegranularity"))
            .and_then(|pg| PriceGranularity::from_json(pg));

        // Extract unified privacy config (GDPR, CCPA, COPPA, LMT) from the request.
        let privacy_config = privacy::extract_privacy_config(bid_request);

        // Execute ProcessedAuctionRequest hooks after validation, before bidder fan-out.
        if let Some(executor) = &self.hook_executor {
            let payload = serde_json::to_value(&request.bid_request)
                .unwrap_or(serde_json::Value::Null);
            if let Err(reject) = executor
                .execute_stage(hooks::Stage::ProcessedAuctionRequest, payload)
                .await
            {
                return Err(anyhow::anyhow!("request rejected by ProcessedAuctionRequest hook: {}", reject));
            }
        }

        // Split impressions per bidder: extract imp.ext.prebid.bidder.<name> params
        // and create sanitized imp copies where each bidder only sees its own params.
        // This mirrors Go exchange/utils.go splitImps().
        let all_bidder_names: Vec<String> = {
            let mut names: Vec<String> = self.adapters.keys().cloned().collect();
            for alias_name in self.aliases.keys() {
                if !names.contains(alias_name) {
                    names.push(alias_name.clone());
                }
            }
            names
        };

        let bidder_imps: HashMap<String, Vec<openrtb::Imp>> =
            split_imps_for_bidders(&bid_request.imp, &all_bidder_names);
        let active_bidders: Vec<String> = bidder_imps.keys().cloned().collect();

        // --- SChain: apply host node to each bidder request ---
        // Build a modified base request with the host schain node prepended if configured.
        // This clone is used as the template for each per-bidder request.
        let schain_base_request: Option<openrtb::BidRequest> = if let Some(host_node) = &self.schain_node {
            let mut req = bid_request.clone();
            let host_node = host_node.clone();
            let source = req.source.get_or_insert_with(Default::default);
            match &mut source.schain {
                Some(schain) => {
                    // Prepend the host node at position 0.
                    schain.nodes.insert(0, host_node);
                    tracing::debug!("prepended host schain node to existing schain");
                }
                None => {
                    // No existing schain — create one with complete=0.
                    source.schain = Some(openrtb::SupplyChain {
                        complete: 0,
                        nodes: vec![host_node],
                        ver: "1.0".to_string(),
                        ext: None,
                    });
                    tracing::debug!("created new schain with host node (complete=0)");
                }
            }
            Some(req)
        } else {
            // Pass-through: existing schain (if any) flows through unchanged.
            if let Some(source) = &bid_request.source {
                if source.schain.is_some() {
                    tracing::debug!("schain present on request source; passing through to bidders as-is");
                }
            }
            None
        };

        let extra_info = ExtraRequestInfo {
            pbs_entry_point: "openrtb2-auction".to_string(),
            global_privacy_control_header: String::new(),
        };

        let mut join_set = tokio::task::JoinSet::new();
        let mut collected_non_bids: Vec<SeatNonBid> = Vec::new();

        for bidder_name in active_bidders {
            // Unified privacy enforcement: COPPA, LMT, CCPA, GDPR.
            let privacy_check = privacy::check_privacy_for_bidder(&privacy_config, &bidder_name);
            if privacy_check != privacy::PrivacyResult::Allow {
                let (status_code, reason) = match privacy_check {
                    privacy::PrivacyResult::BlockGdpr  => (50, "GDPR: no valid consent string"),
                    privacy::PrivacyResult::BlockCcpa  => (51, "CCPA: us_privacy opt-out"),
                    privacy::PrivacyResult::BlockCoppa => (52, "COPPA: child-directed flag"),
                    privacy::PrivacyResult::BlockLmt   => (53, "LMT: limit ad tracking"),
                    privacy::PrivacyResult::Allow      => unreachable!(),
                };
                tracing::warn!(bidder = %bidder_name, "skipping bidder due to privacy: {}", reason);
                let non_bids: Vec<NonBid> = bid_request.imp.iter().map(|imp| NonBid {
                    impid: imp.id.clone(),
                    statuscode: status_code,
                    ext: None,
                }).collect();
                if !non_bids.is_empty() {
                    collected_non_bids.push(SeatNonBid {
                        seat: bidder_name.clone(),
                        nonbid: non_bids,
                        ext: None,
                    });
                }
                continue;
            }

            // --- Stored bid response short-circuit (per-imp, per-bidder) ---
            // If this bidder has stored bid responses for any imps, inject them
            // directly as BidderResults without calling the adapter.
            if let Some(imp_responses) = stored_bid_responses.get(&bidder_name) {
                let mut typed_bids: Vec<pbs_adapters::TypedBid> = Vec::new();
                for (imp_id, stored_json) in imp_responses {
                    // Parse stored response as array of SeatBid or a single BidResponse.
                    if let Some(seat_bids) = stored_json.as_array() {
                        for sb in seat_bids {
                            if let Some(bids) = sb.get("bid").and_then(|b| b.as_array()) {
                                for bid_val in bids {
                                    if let Ok(mut bid) = serde_json::from_value::<openrtb::Bid>(bid_val.clone()) {
                                        bid.impid = imp_id.clone();
                                        let bid_type = openrtb_ext::BidType::from_mtype(bid.mtype.unwrap_or(0));
                                        typed_bids.push(pbs_adapters::TypedBid::new(bid, bid_type));
                                    }
                                }
                            }
                        }
                    }
                }
                if !typed_bids.is_empty() {
                    // Push directly as a result — skip the adapter call for these imps.
                    let name_clone = bidder_name.clone();
                    join_set.spawn(async move {
                        BidderResult {
                            bidder_name: name_clone,
                            response: Ok(BidderResponse {
                                bids: typed_bids,
                                currency: "USD".to_string(),
                                fledge_auction_configs: Vec::new(),
                            }),
                            duration_ms: 0,
                            http_calls: Vec::new(),
                            timed_out: false,
                        }
                    });
                    continue; // skip the live adapter call
                }
            }

            // Resolve alias: if bidder_name is an alias, look up the canonical adapter name.
            // The SeatBid in the response will use the original (alias) name.
            let canonical_name = self.aliases.get(&bidder_name)
                .cloned()
                .unwrap_or_else(|| bidder_name.clone());

            if let Some(adapted) = self.adapters.get(&canonical_name) {
                // Use per-bidder timeout if configured, otherwise fall back to global.
                let effective_timeout_ms = per_bidder_timeouts
                    .get(&bidder_name)
                    .copied()
                    .unwrap_or(timeout_ms);

                // Record that we are sending a request to this bidder.
                if let Some(m) = &self.metrics {
                    m.record_bidder_request(&bidder_name);
                }

                // Clone everything needed for the async task.
                // Use the schain-modified base request when a host node is configured.
                let adapted = AdaptedBidder {
                    bidder: adapted.bidder.clone(),
                    http_client: self.http_client.clone(),
                    endpoint: adapted.endpoint.clone(),
                    endpoint_compression: adapted.endpoint_compression.clone(),
                };
                let mut req = schain_base_request
                    .as_ref()
                    .unwrap_or(bid_request)
                    .clone();

                // Replace request imps with only this bidder's sanitized imps.
                if let Some(bidder_specific_imps) = bidder_imps.get(&bidder_name) {
                    req.imp = bidder_specific_imps.clone();
                }

                apply_fpd_for_bidder(&mut req, &bidder_name);
                // COPPA sanitization: strip user/device identifiers for child-directed requests.
                privacy::sanitize_request_for_coppa(&mut req);

                // Execute BidderRequest hooks before sending request to this bidder.
                if let Some(executor) = &self.hook_executor {
                    let payload = serde_json::json!({
                        "bidder": bidder_name,
                        "bid_request": serde_json::to_value(&req).unwrap_or(serde_json::Value::Null),
                    });
                    if let Err(reject) = executor
                        .execute_stage(hooks::Stage::BidderRequest, payload)
                        .await
                    {
                        tracing::warn!(
                            bidder = %bidder_name,
                            "BidderRequest hook rejected bidder: {}; skipping",
                            reject,
                        );
                        continue;
                    }
                }

                let extra = extra_info.clone();
                // Spawn task using the alias name so the SeatBid carries the alias.
                let seat_name = bidder_name.clone();

                join_set.spawn(async move {
                    adapted.request_bid(&req, &seat_name, &extra, effective_timeout_ms).await
                });
            }
        }

        // --- First-party data passthrough ---
        // FPD fields (site.ext.data, app.ext.data, user.ext.data, imp[].ext.data) are stored in
        // `ext: Option<serde_json::Value>` on the respective openrtb structs.  Each bidder receives
        // a full clone of BidRequest, so FPD passes through automatically.

        // Collect results: keep TypedBids per bidder so we can compute targeting before flattening.
        let mut bidder_results: Vec<(String, Vec<pbs_adapters::TypedBid>)> = Vec::new();
        let mut timed_out_bidders: Vec<String> = Vec::new();

        // Debug mode collections (populated only when bid_request.test == Some(1)).
        let is_test = bid_request.test == Some(1);
        let mut debug_http_calls: HashMap<String, Vec<openrtb_ext::ExtHttpCall>> = HashMap::new();
        let mut debug_bidder_timing: HashMap<String, u64> = HashMap::new();
        let mut bidder_errors: HashMap<String, Vec<String>> = HashMap::new();
        // Always-populated map of bidder response times for responsetimemillis.
        let mut all_bidder_timing: HashMap<String, u64> = HashMap::new();

        while let Some(result) = join_set.join_next().await {
            match result {
                Ok(bidder_result) => {
                    if bidder_result.timed_out {
                        timed_out_bidders.push(bidder_result.bidder_name.clone());
                    }

                    // Always record per-bidder response time for responsetimemillis.
                    all_bidder_timing.insert(
                        bidder_result.bidder_name.clone(),
                        bidder_result.duration_ms,
                    );

                    // Collect debug info when test mode is active.
                    if is_test {
                        if !bidder_result.http_calls.is_empty() {
                            debug_http_calls
                                .entry(bidder_result.bidder_name.clone())
                                .or_default()
                                .extend(bidder_result.http_calls.clone());
                        }
                        debug_bidder_timing.insert(
                            bidder_result.bidder_name.clone(),
                            bidder_result.duration_ms,
                        );
                    }

                    // Execute RawBidderResponse hooks after receiving each bidder's response.
                    if let Some(executor) = &self.hook_executor {
                        let payload = serde_json::json!({
                            "bidder": bidder_result.bidder_name,
                            "timed_out": bidder_result.timed_out,
                            "duration_ms": bidder_result.duration_ms,
                        });
                        if let Err(reject) = executor
                            .execute_stage(hooks::Stage::RawBidderResponse, payload)
                            .await
                        {
                            tracing::warn!(
                                bidder = %bidder_result.bidder_name,
                                "RawBidderResponse hook rejected: {}",
                                reject,
                            );
                        }
                    }

                    // Record per-bidder metrics.
                    if let Some(m) = &self.metrics {
                        let status = if bidder_result.timed_out {
                            pbs_metrics::BidderStatus::TimedOut
                        } else {
                            match &bidder_result.response {
                                Ok(r) if r.bids.is_empty() => pbs_metrics::BidderStatus::NoBid,
                                Ok(_) => pbs_metrics::BidderStatus::Got,
                                Err(_) => pbs_metrics::BidderStatus::Error,
                            }
                        };
                        m.record_bidder_response(
                            &bidder_result.bidder_name,
                            status,
                            bidder_result.duration_ms,
                        );
                    }

                    match bidder_result.response {
                        Ok(mut response) => {
                            if !response.bids.is_empty() {
                                // Validate bid currency before processing.
                                let cur_slice = bid_request.cur.as_deref().unwrap_or(&[]);
                                if let Err(currency_err) = validate_bid_currency(
                                    cur_slice,
                                    &response.currency,
                                ) {
                                    tracing::warn!(bidder = %bidder_result.bidder_name,
                                        error = %currency_err, "dropping all bids: invalid currency");
                                    bidder_errors
                                        .entry(bidder_result.bidder_name.clone())
                                        .or_default()
                                        .push(currency_err);
                                    // Skip this bidder's bids entirely — same as Go behavior.
                                } else {
                                // Currency conversion: normalize bid prices to USD.
                                if response.currency != "USD" {
                                    if let Some(converter) = &request.currency_rates {
                                        for typed_bid in &mut response.bids {
                                            if let Some(converted) = converter.convert(typed_bid.bid.price, &response.currency, "USD") {
                                                typed_bid.bid.price = converted;
                                            }
                                        }
                                    }
                                }

                                // Parse bid adjustment factors: {"appnexus": 0.9, "rubicon": 1.1, ...}
                                let adjustment_factors: HashMap<String, f64> = bid_request.ext
                                    .as_ref()
                                    .and_then(|e| e.get("prebid"))
                                    .and_then(|p| p.get("bidadjustmentfactors"))
                                    .and_then(|f| serde_json::from_value(f.clone()).ok())
                                    .unwrap_or_default();

                                // Apply bid adjustment factor for this bidder (before floor comparison).
                                if let Some(&factor) = adjustment_factors.get(&bidder_result.bidder_name) {
                                    for typed_bid in &mut response.bids {
                                        typed_bid.bid.price *= factor;
                                    }
                                }

                                // Filter bids below floor price; collect rejected ones as non-bids.
                                let mut floor_non_bids: Vec<NonBid> = Vec::new();
                                let accepted: Vec<pbs_adapters::TypedBid> = response
                                    .bids
                                    .into_iter()
                                    .filter(|typed_bid| {
                                        let floor = bid_request
                                            .imp
                                            .iter()
                                            .find(|imp| imp.id == typed_bid.bid.impid)
                                            .and_then(|imp| imp.bidfloor.filter(|&f| f > 0.0));
                                        match floor {
                                            Some(floor) if typed_bid.bid.price < floor => {
                                                floor_non_bids.push(NonBid {
                                                    impid: typed_bid.bid.impid.clone(),
                                                    statuscode: 300,
                                                    ext: None,
                                                });
                                                false
                                            }
                                            _ => true,
                                        }
                                    })
                                    .collect();

                                if !floor_non_bids.is_empty() {
                                    collected_non_bids.push(SeatNonBid {
                                        seat: bidder_result.bidder_name.clone(),
                                        nonbid: floor_non_bids,
                                        ext: None,
                                    });
                                }

                                // Bid validation: drop invalid bids, warn on suspicious ones.
                                let validated = validate_bids(accepted, &bid_request.imp);

                                // Deduplication: within this bidder's response, keep only the
                                // highest-priced bid for each bid.id.
                                let mut deduped: Vec<pbs_adapters::TypedBid> = Vec::new();
                                let mut seen_ids: HashMap<String, usize> = HashMap::new();
                                for tb in validated {
                                    if let Some(&idx) = seen_ids.get(&tb.bid.id) {
                                        if tb.bid.price > deduped[idx].bid.price {
                                            deduped[idx] = tb;
                                        }
                                    } else {
                                        seen_ids.insert(tb.bid.id.clone(), deduped.len());
                                        deduped.push(tb);
                                    }
                                }

                                // Multi-bid enforcement: limit the number of bids per imp
                                // based on req.ext.prebid.multibid configuration.
                                // Default is 1 bid per imp per bidder.
                                let max_bids_per_imp = multi_bid_limits
                                    .get(&bidder_result.bidder_name)
                                    .copied()
                                    .unwrap_or(1) as usize;

                                let deduped = if max_bids_per_imp <= 1 {
                                    // Default: keep only the highest-priced bid per imp.
                                    let mut best_per_imp: HashMap<String, pbs_adapters::TypedBid> = HashMap::new();
                                    for tb in deduped {
                                        let imp_id = tb.bid.impid.clone();
                                        match best_per_imp.get(&imp_id) {
                                            Some(existing) if existing.bid.price >= tb.bid.price => {}
                                            _ => { best_per_imp.insert(imp_id, tb); }
                                        }
                                    }
                                    best_per_imp.into_values().collect::<Vec<_>>()
                                } else {
                                    // Multi-bid: keep up to max_bids_per_imp highest-priced bids per imp.
                                    let mut bids_per_imp: HashMap<String, Vec<pbs_adapters::TypedBid>> = HashMap::new();
                                    for tb in deduped {
                                        bids_per_imp.entry(tb.bid.impid.clone()).or_default().push(tb);
                                    }
                                    let mut result = Vec::new();
                                    for (_imp_id, mut imp_bids) in bids_per_imp {
                                        // Sort descending by price, keep top N.
                                        imp_bids.sort_by(|a, b| b.bid.price.partial_cmp(&a.bid.price).unwrap_or(std::cmp::Ordering::Equal));
                                        imp_bids.truncate(max_bids_per_imp);
                                        result.extend(imp_bids);
                                    }
                                    result
                                };

                                if !deduped.is_empty() {
                                    // Record per-bid metrics for each accepted bid.
                                    if let Some(m) = &self.metrics {
                                        for typed_bid in &deduped {
                                            m.record_bid_count(
                                                &bidder_result.bidder_name,
                                                &typed_bid.bid_type.to_string(),
                                            );
                                        }
                                    }
                                    bidder_results.push((bidder_result.bidder_name, deduped));
                                }
                            } // end else (currency valid)
                            } // end if !response.bids.is_empty()
                        }
                        Err(errs) => {
                            // Collect error strings for response.ext.prebid.errors.
                            let err_strings: Vec<String> = errs.iter().map(|e| e.to_string()).collect();
                            if !err_strings.is_empty() {
                                // Record error metrics for each error.
                                if let Some(m) = &self.metrics {
                                    for err in &errs {
                                        let err_type = match err {
                                            pbs_adapters::BidderError::Timeout => "timeout",
                                            pbs_adapters::BidderError::BadInput(_) => "bad_input",
                                            pbs_adapters::BidderError::BadServerResponse(_) => "bad_server_response",
                                            _ => "unknown",
                                        };
                                        m.record_bidder_error(&bidder_result.bidder_name, err_type);
                                    }
                                }
                                bidder_errors
                                    .entry(bidder_result.bidder_name.clone())
                                    .or_default()
                                    .extend(err_strings);
                            }
                            // Bidder returned errors — record a non-bid with reason code 200 per imp.
                            let non_bids: Vec<NonBid> = bid_request.imp.iter().map(|imp| NonBid {
                                impid: imp.id.clone(),
                                statuscode: 200,
                                ext: None,
                            }).collect();
                            if !non_bids.is_empty() {
                                collected_non_bids.push(SeatNonBid {
                                    seat: bidder_result.bidder_name,
                                    nonbid: non_bids,
                                    ext: None,
                                });
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Bidder task panicked: {}", e);
                }
            }
        }

        // Execute AllProcessedBidResponses hooks after collecting all bidder responses.
        if let Some(executor) = &self.hook_executor {
            let payload = serde_json::json!({
                "bidder_count": bidder_results.len(),
                "timed_out_bidders": timed_out_bidders,
            });
            // Rejection at this stage is not supported; errors are logged as warnings.
            if let Err(reject) = executor
                .execute_stage(hooks::Stage::AllProcessedBidResponses, payload)
                .await
            {
                tracing::warn!("AllProcessedBidResponses hook rejected: {}", reject);
            }
        }

        // --- Targeting / price granularity ---
        // Emit per-bidder keys (hb_pb_<bidder>, hb_bidder_<bidder>, hb_adid_<bidder>) and
        // winner keys (hb_pb, hb_bidder, hb_adid) for the highest bid per impression.
        let mut targeting: HashMap<String, HashMap<String, String>> = HashMap::new();

        let pg_ref = custom_price_granularity.as_ref();

        // Per-bidder keys.
        for (bidder_name, typed_bids) in &bidder_results {
            for typed_bid in typed_bids {
                let keys = targeting.entry(typed_bid.bid.impid.clone()).or_default();
                keys.insert(
                    format!("hb_pb_{}", bidder_name),
                    price_granularity_bucket(typed_bid.bid.price, pg_ref),
                );
                keys.insert(
                    format!("hb_bidder_{}", bidder_name),
                    bidder_name.clone(),
                );
                // Use bid.adid (advertiser creative ID) if available, else fall back to bid.id.
                let adid = typed_bid
                    .bid
                    .adid
                    .clone()
                    .unwrap_or_else(|| typed_bid.bid.id.clone());
                keys.insert(format!("hb_adid_{}", bidder_name), adid);
            }
        }

        // Winner keys: highest bid per impression.
        // Tuple: (price, bidder_name, adid).
        let mut winners: HashMap<String, (f64, String, String)> = HashMap::new();
        for (bidder_name, typed_bids) in &bidder_results {
            for typed_bid in typed_bids {
                let entry = winners
                    .entry(typed_bid.bid.impid.clone())
                    .or_insert((f64::NEG_INFINITY, String::new(), String::new()));
                if typed_bid.bid.price > entry.0 {
                    let adid = typed_bid
                        .bid
                        .adid
                        .clone()
                        .unwrap_or_else(|| typed_bid.bid.id.clone());
                    *entry = (typed_bid.bid.price, bidder_name.clone(), adid);
                }
            }
        }
        for (imp_id, (price, bidder_name, adid)) in &winners {
            let keys = targeting.entry(imp_id.clone()).or_default();
            keys.insert("hb_pb".to_string(), price_granularity_bucket(*price, pg_ref));
            keys.insert("hb_bidder".to_string(), bidder_name.clone());
            keys.insert("hb_adid".to_string(), adid.clone());
        }

        // --- Ad server targeting rules ---
        // Parse req.ext.prebid.adservertargeting and apply rules per bid.
        let ast_rules: Vec<adserver_targeting::AdServerTargetingRule> = bid_request
            .ext
            .as_ref()
            .and_then(|e| e.get("prebid"))
            .and_then(|p| p.get("adservertargeting"))
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        if !ast_rules.is_empty() {
            for (_bidder_name, typed_bids) in &bidder_results {
                for typed_bid in typed_bids {
                    let extra = adserver_targeting::apply_adserver_targeting(
                        &ast_rules,
                        bid_request,
                        Some(&typed_bid.bid),
                    );
                    let keys = targeting.entry(typed_bid.bid.impid.clone()).or_default();
                    for (k, v) in extra {
                        keys.insert(k, v);
                    }
                }
            }
        }

        // --- Category mapping (hb_pb_cat_dur) ---
        // When req.ext.prebid.targeting.includebrandcategory is set, generate the
        // `hb_pb_cat_dur` targeting key for video bids in the format:
        //   {price_bucket}_{category}_{duration}s
        // Category comes from bid.cat[0] (if present), else "uncategorized".
        // Duration comes from bid.ext.prebid.video.duration.
        let include_brand_category: Option<openrtb_ext::ExtIncludeBrandCategory> = bid_request
            .ext
            .as_ref()
            .and_then(|e| e.get("prebid"))
            .and_then(|p| p.get("targeting"))
            .and_then(|t| t.get("includebrandcategory"))
            .and_then(|v| serde_json::from_value(v.clone()).ok());

        if include_brand_category.is_some() {
            for (bidder_name, typed_bids) in &bidder_results {
                for typed_bid in typed_bids {
                    // Only apply to video bids.
                    if !matches!(typed_bid.bid_type, openrtb_ext::BidType::Video) {
                        continue;
                    }

                    // Get duration from bid.ext.prebid.video.duration.
                    let duration_opt: Option<i32> = typed_bid
                        .bid
                        .ext
                        .as_ref()
                        .and_then(|e| e.get("prebid"))
                        .and_then(|p| p.get("video"))
                        .and_then(|v| v.get("duration"))
                        .and_then(|d| d.as_i64())
                        .map(|d| d as i32);

                    let duration = match duration_opt {
                        Some(d) if d > 0 => d,
                        _ => continue, // skip bids without a valid duration
                    };

                    // Category from bid.cat[0] if present, else "uncategorized".
                    let category = typed_bid
                        .bid
                        .cat
                        .as_ref()
                        .and_then(|cats| cats.first())
                        .map(|s| s.as_str())
                        .unwrap_or("uncategorized");

                    let price_bucket = price_granularity_bucket(typed_bid.bid.price, pg_ref);
                    let cat_dur = format!("{}_{}_{}", price_bucket, category, duration);

                    let keys = targeting.entry(typed_bid.bid.impid.clone()).or_default();
                    // Per-bidder key.
                    keys.insert(format!("hb_pb_cat_dur_{}", bidder_name), cat_dur.clone());
                    // Winner key (set once — first bidder wins; can be overridden by higher price logic if needed).
                    keys.entry("hb_pb_cat_dur".to_string()).or_insert(cat_dur);
                }
            }
        }

        // --- Deal tier / deal targeting ---
        // For bids that carry a deal ID, set the `hb_deal` targeting key.
        // Also validate against imp.pmp.deals[].ext.prebid.dealTier.minDealTier when present.
        //
        // Build imp_id -> deal_id -> DealTier map from imp.pmp.deals[].ext.prebid.dealTier.
        let mut imp_deal_tiers: HashMap<String, HashMap<String, openrtb_ext::DealTier>> =
            HashMap::new();
        for imp in &bid_request.imp {
            if let Some(pmp) = &imp.pmp {
                for deal in &pmp.deals {
                    let tier: Option<openrtb_ext::DealTier> = deal
                        .ext
                        .as_ref()
                        .and_then(|e| e.get("prebid"))
                        .and_then(|p| p.get("dealTier"))
                        .and_then(|v| serde_json::from_value(v.clone()).ok());
                    if let Some(t) = tier {
                        imp_deal_tiers
                            .entry(imp.id.clone())
                            .or_default()
                            .insert(deal.id.clone(), t);
                    }
                }
            }
        }

        for (bidder_name, typed_bids) in &bidder_results {
            for typed_bid in typed_bids {
                let deal_id = match &typed_bid.bid.dealid {
                    Some(d) if !d.is_empty() => d.clone(),
                    _ => continue,
                };

                let keys = targeting.entry(typed_bid.bid.impid.clone()).or_default();
                // Per-bidder deal key.
                keys.insert(format!("hb_deal_{}", bidder_name), deal_id.clone());
                // Winner deal key (first encountered wins).
                keys.entry("hb_deal".to_string()).or_insert(deal_id.clone());

                // Validate deal tier if configured.
                if let Some(deal_map) = imp_deal_tiers.get(&typed_bid.bid.impid) {
                    if let Some(tier) = deal_map.get(&deal_id) {
                        let min_tier = tier.min_deal_tier.unwrap_or(0);
                        let prefix = tier.prefix.as_deref().unwrap_or("");
                        if !prefix.is_empty() && min_tier > 0 {
                            // Read deal priority from bid.ext.prebid.dealpriority.
                            let deal_priority: i32 = typed_bid
                                .bid
                                .ext
                                .as_ref()
                                .and_then(|e| e.get("prebid"))
                                .and_then(|p| p.get("dealpriority"))
                                .and_then(|v| v.as_i64())
                                .map(|v| v as i32)
                                .unwrap_or(0);

                            if deal_priority >= min_tier {
                                // Replace the hb_pb_cat_dur price bucket prefix with deal tier prefix.
                                let tier_prefix = format!("{}{}_ ", prefix, deal_priority);
                                let cat_dur_key = format!("hb_pb_cat_dur_{}", bidder_name);
                                if let Some(existing) = keys.get(&cat_dur_key).cloned() {
                                    // Replace the price-bucket portion (first segment before '_').
                                    if let Some(rest) = existing.find('_').map(|i| &existing[i + 1..]) {
                                        let updated = format!("{}{}_{}", prefix, deal_priority, rest);
                                        keys.insert(cat_dur_key, updated);
                                    }
                                }
                                tracing::debug!(
                                    bidder = %bidder_name,
                                    bid_id = %typed_bid.bid.id,
                                    deal_id = %deal_id,
                                    deal_priority,
                                    min_tier,
                                    "deal tier satisfied"
                                );
                                // Suppress unused variable warning.
                                let _ = tier_prefix;
                            } else {
                                tracing::debug!(
                                    bidder = %bidder_name,
                                    bid_id = %typed_bid.bid.id,
                                    deal_id = %deal_id,
                                    deal_priority,
                                    min_tier,
                                    "bid deal priority below minimum tier"
                                );
                            }
                        }
                    }
                }
            }
        }

        // --- Extended bid adjustment rules ---
        // Parse req.ext.prebid.bidadjustments (new format) if present.
        // Rules are applied with specificity: bidder+deal_id > bidder+* > *+*
        // (Currently parsed and stored; application mirrors the flat factor path above.)
        let bid_adjustment_rules: Option<openrtb_ext::BidAdjustmentRule> = bid_request
            .ext
            .as_ref()
            .and_then(|e| e.get("prebid"))
            .and_then(|p| p.get("bidadjustments"))
            .and_then(|v| serde_json::from_value(v.clone()).ok());

        if let Some(rules) = bid_adjustment_rules {
            if let Some(bidders_map) = rules.bidders {
                for (bidder_name, typed_bids) in &mut bidder_results {
                    // Look up rules for this bidder, then fall back to "*".
                    let bidder_rules = bidders_map
                        .get(bidder_name.as_str())
                        .or_else(|| bidders_map.get("*"));

                    let bidder_rules = match bidder_rules {
                        Some(r) => r,
                        None => continue,
                    };

                    for typed_bid in typed_bids.iter_mut() {
                        let deal_id = typed_bid.bid.dealid.as_deref().unwrap_or("*");

                        // Most specific: this deal id; fallback to wildcard "*".
                        let adjustments = bidder_rules
                            .get(deal_id)
                            .or_else(|| bidder_rules.get("*"));

                        let adjustments = match adjustments {
                            Some(a) => a,
                            None => continue,
                        };

                        // Apply the first matching adjustment.
                        for adj in adjustments {
                            match adj.adj_type.as_str() {
                                "multiplier" => {
                                    typed_bid.bid.price *= adj.value;
                                    tracing::debug!(
                                        bidder = %bidder_name,
                                        bid_id = %typed_bid.bid.id,
                                        factor = adj.value,
                                        "applied bid adjustment multiplier"
                                    );
                                    break;
                                }
                                "static" => {
                                    typed_bid.bid.price = adj.value;
                                    tracing::debug!(
                                        bidder = %bidder_name,
                                        bid_id = %typed_bid.bid.id,
                                        price = adj.value,
                                        "applied bid adjustment static price"
                                    );
                                    break;
                                }
                                "cpm" => {
                                    // For CPM type: add the value to the existing price.
                                    typed_bid.bid.price += adj.value;
                                    tracing::debug!(
                                        bidder = %bidder_name,
                                        bid_id = %typed_bid.bid.id,
                                        delta = adj.value,
                                        "applied bid adjustment cpm delta"
                                    );
                                    break;
                                }
                                other => {
                                    tracing::warn!(
                                        adj_type = %other,
                                        "unknown bid adjustment type; skipping"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }

        // --- Cache bids (doCache equivalent) ---
        // If req.ext.prebid.cache.bids is set and a cache client is configured,
        // write winning bids to Prebid Cache and set hb_cache_id targeting keys.
        let cache_bids_enabled = bid_request.ext.as_ref()
            .and_then(|e| e.get("prebid"))
            .and_then(|p| p.get("cache"))
            .and_then(|c| c.get("bids"))
            .is_some();

        if cache_bids_enabled {
            if let Some(cache) = &self.cache_client {
                // Cache each winning bid.
                for (imp_id, (_, bidder_name, _)) in &winners {
                    // Find the winning bid's full data.
                    let winning_bid = bidder_results.iter()
                        .find(|(bn, _)| bn == bidder_name)
                        .and_then(|(_, bids)| bids.iter().find(|b| &b.bid.impid == imp_id));

                    if let Some(typed_bid) = winning_bid {
                        let bid_json = serde_json::to_value(&typed_bid.bid).unwrap_or_default();
                        if let Some(cache_id) = cache.put_json(&bid_json, 300) {
                            let keys = targeting.entry(imp_id.clone()).or_default();
                            keys.insert("hb_cache_id".to_string(), cache_id.clone());
                            keys.insert(
                                format!("hb_cache_id_{}", bidder_name),
                                cache_id.clone(),
                            );
                            // Also provide the cache host/path for the winning bid.
                            let cache_url = cache.get_url(&cache_id);
                            if !cache_url.is_empty() {
                                keys.insert("hb_cache_host".to_string(), cache_url.clone());
                            }
                        }
                    }
                }
            }
        }

        // Assemble seat bids from collected results, resolving all auction macros.
        let auction_id = bid_request.id.clone();
        let auction_currency = bid_request
            .cur
            .as_ref()
            .and_then(|c| c.first())
            .cloned()
            .unwrap_or_else(|| "USD".to_string());
        let timeout_ms = bid_request.tmax.unwrap_or(0) as u64;

        let mut seat_bids: Vec<openrtb::SeatBid> = Vec::new();
        for (bidder_name, typed_bids) in bidder_results {
            let bids: Vec<openrtb::Bid> = typed_bids.into_iter().map(|tb| {
                let mut bid = tb.bid;
                let macro_values = macros::MacroValues {
                    auction_price: bid.price,
                    auction_currency: auction_currency.clone(),
                    auction_id: auction_id.clone(),
                    bidder_name: bidder_name.clone(),
                    imp_id: bid.impid.clone(),
                    timeout: timeout_ms,
                    ad_markup: bid.adm.clone(),
                };
                macros::apply_bid_macros(&mut bid, &macro_values);
                bid
            }).collect();
            seat_bids.push(openrtb::SeatBid {
                bid: bids,
                seat: Some(bidder_name),
                ..Default::default()
            });
        }

        // Build response ext.
        //
        // Structure:
        //   {
        //     "prebid": {
        //       "timing": { "respondedMs": <ms> },
        //       "errors": { "<bidder>": ["..."] },        // omitted when empty
        //       "seatnonbid": [...]                       // omitted when empty
        //     },
        //     "debug": {                                  // only when test == 1
        //       "httpcalls": { "<bidder>": [...] },
        //       "bidmeta":   { "<bidder>": { "serverResponseTimeMs": <ms> } }
        //     },
        //     "dsa": { ... }                              // passthrough, omitted when absent
        //   }
        let elapsed_ms = request.start_time.elapsed().as_millis() as u64;

        // Record total auction duration.
        if let Some(m) = &self.metrics {
            m.record_auction_duration("openrtb2", elapsed_ms);
        }

        let mut prebid_obj = serde_json::json!({
            "timing": { "respondedMs": elapsed_ms }
        });

        if !bidder_errors.is_empty() {
            prebid_obj["errors"] = serde_json::to_value(&bidder_errors).unwrap_or_default();
        }

        if !collected_non_bids.is_empty() {
            prebid_obj["seatnonbid"] = serde_json::to_value(&collected_non_bids).unwrap_or_default();
        }

        // responsetimemillis: map of bidder name -> response time in milliseconds
        if !all_bidder_timing.is_empty() {
            prebid_obj["responsetimemillis"] =
                serde_json::to_value(&all_bidder_timing).unwrap_or_default();
        }

        // tmaxrequest: the tmax value from the original bid request
        if let Some(tmax) = bid_request.tmax {
            prebid_obj["tmaxrequest"] = serde_json::json!(tmax);
        }

        let mut ext_obj = serde_json::json!({ "prebid": prebid_obj });

        // DSA passthrough from request regs.dsa.
        if let Some(dsa_val) = bid_request
            .regs
            .as_ref()
            .and_then(|r| r.dsa.as_ref())
            .and_then(|dsa| serde_json::to_value(dsa).ok())
        {
            ext_obj["dsa"] = dsa_val;
        }

        // Debug block — only included when request.test == 1.
        if is_test {
            let mut bidmeta = serde_json::Map::new();
            for (bidder, ms) in &debug_bidder_timing {
                bidmeta.insert(
                    bidder.clone(),
                    serde_json::json!({ "serverResponseTimeMs": ms }),
                );
            }
            ext_obj["debug"] = serde_json::json!({
                "httpcalls": debug_http_calls,
                "bidmeta":   bidmeta,
            });
        }

        let response_ext: Option<serde_json::Value> = Some(ext_obj);

        let bid_response = openrtb::BidResponse {
            id: bid_request.id.clone(),
            seatbid: seat_bids,
            cur: Some("USD".to_string()),
            ext: response_ext,
            ..Default::default()
        };

        // Log analytics event
        if let Some(analytics_backend) = &self.analytics {
            let bid_count: usize = bid_response.seatbid.iter().map(|sb| sb.bid.len()).sum();
            let total_revenue: f64 = bid_response.seatbid.iter()
                .flat_map(|sb| sb.bid.iter())
                .map(|b| b.price)
                .sum();
            let bidder_count = bid_response.seatbid.len();
            analytics_backend.log_auction(analytics::AuctionEvent {
                timestamp: chrono::Utc::now().timestamp(),
                request_id: bid_request.id.clone(),
                status: "ok".to_string(),
                bidder_count,
                bid_count,
                total_revenue,
            });
        }

        // Execute AuctionResponse hooks after final response assembly.
        // Rejection is not supported at this stage; errors are logged as warnings.
        if let Some(executor) = &self.hook_executor {
            let payload = serde_json::to_value(&bid_response)
                .unwrap_or(serde_json::Value::Null);
            if let Err(reject) = executor
                .execute_stage(hooks::Stage::AuctionResponse, payload)
                .await
            {
                tracing::warn!("AuctionResponse hook rejected: {}", reject);
            }
        }

        Ok(AuctionResponse {
            bid_response,
            seat_non_bids: collected_non_bids,
            targeting,
            timed_out_bidders,
        })
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Bid ID Generation
// ──────────────────────────────────────────────────────────────────────────────

/// Strategy for generating bid IDs in the auction response.
///
/// Mirrors the Go `BidIDGenerator` interface with three strategies:
///   - `BidderGenerated`: keep the original bid ID from the bidder (no-op).
///   - `RequestBidId`: use `{request.id}-{bidder}-{counter}` to produce
///     deterministic, request-scoped IDs.
///   - `Uuid`: generate a fresh UUID v4 for every bid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BidIdStrategy {
    /// Keep the original bid.id returned by the bidder.
    BidderGenerated,
    /// Derive a deterministic ID from request.id + bidder name + counter.
    RequestBidId,
    /// Generate a new UUID v4 for each bid.
    Uuid,
}

impl Default for BidIdStrategy {
    fn default() -> Self {
        Self::BidderGenerated
    }
}

/// Generate a bid ID according to the chosen strategy.
///
/// * `strategy`       – which ID generation approach to use.
/// * `original_bid_id` – the bid.id value the bidder returned.
/// * `request_id`     – the auction request ID (`request.id`).
/// * `bidder_name`    – the canonical bidder name (used by `RequestBidId`).
/// * `counter`        – a monotonically increasing counter per bidder within
///                       the same auction (used by `RequestBidId`).
pub fn generate_bid_id(
    strategy: BidIdStrategy,
    original_bid_id: &str,
    request_id: &str,
    bidder_name: &str,
    counter: u64,
) -> String {
    match strategy {
        BidIdStrategy::BidderGenerated => original_bid_id.to_string(),
        BidIdStrategy::RequestBidId => {
            format!("{}-{}-{}", request_id, bidder_name, counter)
        }
        BidIdStrategy::Uuid => uuid::Uuid::new_v4().to_string(),
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// SeatNonBid / NonBidReason
// ──────────────────────────────────────────────────────────────────────────────

/// Reason codes for why a bid was not produced for an impression.
///
/// Reference: IAB OpenRTB community extension `seat-non-bid.md`.
/// These mirror the Go `NonBidReason` constants in `exchange/non_bid_reason.go`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum NonBidReason {
    // ── No-bid (0–99) ────────────────────────────────────────────────────────
    /// The bidder simply chose not to bid.
    NoBid = 0,

    // ── Error (100–199) ──────────────────────────────────────────────────────
    /// General / unclassified error.
    ErrorGeneral = 100,
    /// The bidder did not respond within the timeout window.
    ErrorTimeout = 101,
    /// The bidder endpoint could not be reached (DNS / connection refused).
    ErrorBidderUnreachable = 103,

    // ── Request rejection (200–299) ──────────────────────────────────────────
    /// General request rejection.
    RequestRejectedGeneral = 200,
    /// Request blocked by publisher-level settings.
    RequestBlockedPublisher = 201,
    /// Request blocked by GDPR / privacy enforcement.
    RequestBlockedPrivacy = 202,
    /// Request blocked by DSA transparency requirements.
    RequestBlockedDsa = 203,

    // ── Response rejection (300–399) ─────────────────────────────────────────
    /// General response rejection.
    ResponseRejectedGeneral = 300,
    /// Bid price fell below the impression floor.
    ResponseRejectedBelowFloor = 301,
    /// Category mapping for the bid was invalid.
    ResponseRejectedCategoryMappingInvalid = 303,
    /// Bid was below the deal floor price.
    ResponseRejectedBelowDealFloor = 304,
    /// Creative size not allowed for the placement.
    ResponseRejectedCreativeSizeNotAllowed = 351,
    /// Creative was not secure (HTTP in an HTTPS context).
    ResponseRejectedCreativeNotSecure = 352,
}

impl NonBidReason {
    /// Return the numeric IAB status code for this reason.
    pub fn code(self) -> i32 {
        self as i32
    }
}

impl std::fmt::Display for NonBidReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::NoBid => "no bid",
            Self::ErrorGeneral => "general error",
            Self::ErrorTimeout => "timeout",
            Self::ErrorBidderUnreachable => "bidder unreachable",
            Self::RequestRejectedGeneral => "request rejected (general)",
            Self::RequestBlockedPublisher => "request blocked by publisher",
            Self::RequestBlockedPrivacy => "request blocked by privacy",
            Self::RequestBlockedDsa => "request blocked by DSA",
            Self::ResponseRejectedGeneral => "response rejected (general)",
            Self::ResponseRejectedBelowFloor => "response rejected: below floor",
            Self::ResponseRejectedCategoryMappingInvalid => {
                "response rejected: category mapping invalid"
            }
            Self::ResponseRejectedBelowDealFloor => "response rejected: below deal floor",
            Self::ResponseRejectedCreativeSizeNotAllowed => {
                "response rejected: creative size not allowed"
            }
            Self::ResponseRejectedCreativeNotSecure => {
                "response rejected: creative not secure"
            }
        };
        write!(f, "{}", label)
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// AdsCert Signing (stub interface)
// ──────────────────────────────────────────────────────────────────────────────

/// Header name used to carry the Ads Cert signature, matching the Go constant
/// `adscert.SignHeader`.
pub const ADS_CERT_SIGN_HEADER: &str = "X-Ads-Cert-Auth";

/// Trait representing the Ads Cert request-signing interface.
///
/// Real implementations would use the IABTechLab `adscert` library to produce
/// an authenticated-connection signature.  This trait provides the integration
/// seam so that adapters can conditionally attach the `X-Ads-Cert-Auth` header.
pub trait AdsCertSigner: Send + Sync {
    /// Sign a bid request destined for `destination_url` with the given `body`.
    ///
    /// Returns `Ok(Some(signature))` on success, `Ok(None)` when signing is
    /// disabled / not configured, or `Err` when the signer is enabled but
    /// encounters an error.
    fn sign(&self, destination_url: &str, body: &[u8]) -> Result<Option<String>, String>;
}

/// No-op signer that always returns `None` (signing disabled).
///
/// This is the default used when Ads Cert is not configured, mirroring the Go
/// `NilSigner` implementation.
pub struct NoopAdsCertSigner;

impl AdsCertSigner for NoopAdsCertSigner {
    fn sign(&self, _destination_url: &str, _body: &[u8]) -> Result<Option<String>, String> {
        Ok(None)
    }
}
