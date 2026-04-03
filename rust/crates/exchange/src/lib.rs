use std::collections::HashMap;
use std::sync::Arc;

use openrtb_ext::{NonBid, SeatNonBid};
use pbs_adapters::{BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData};

pub mod analytics;
pub mod currency;
pub mod floors;

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

/// Apply first-party data (FPD) overrides for a specific bidder.
///
/// Reads `req.ext.prebid.data.bidderspecific.<bidder_name>` and merges any
/// `site`, `user` fields it finds into the top-level BidRequest fields.
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

    // Merge site FPD
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
            }
        }
    }

    // Merge user FPD
    if let Some(user_fpd) = fpd.get("user") {
        if let Ok(user_override) = serde_json::from_value::<openrtb::User>(user_fpd.clone()) {
            if let Some(user) = &mut req.user {
                if user_override.buyeruid.is_some() {
                    user.buyeruid = user_override.buyeruid;
                }
                if let Some(new_ext) = user_override.ext {
                    if let Some(existing) = &mut user.ext {
                        if let (Some(obj), Some(new_obj)) =
                            (existing.as_object_mut(), new_ext.as_object())
                        {
                            for (k, v) in new_obj {
                                obj.insert(k.clone(), v.clone());
                            }
                        }
                    } else {
                        user.ext = Some(new_ext);
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

/// Resolve `${AUCTION_PRICE}` macro in a string with the given price.
fn resolve_price_macro(s: &str, price: f64) -> String {
    s.replace("${AUCTION_PRICE}", &format!("{:.4}", price))
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
///   - `bid.id` empty → drop + warn
///   - `bid.impid` does not match any request imp → drop
///   - `bid.price` < 0 → drop
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

/// Exchange orchestrates the full auction: fan-out to bidders, collect results, build response.
pub struct Exchange {
    pub adapters: HashMap<String, AdaptedBidder>,
    pub http_client: reqwest::Client,
    pub metrics: Option<Arc<dyn pbs_metrics::MetricsEngine>>,
    pub analytics: Option<Arc<dyn analytics::AnalyticsBackend>>,
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
        per_bidder_timeouts: &HashMap<String, u64>,
    ) -> Result<AuctionResponse, anyhow::Error> {
        let bid_request = &request.bid_request;

        // Validate impression count
        if bid_request.imp.is_empty() {
            return Err(anyhow::anyhow!("request.imp must contain at least one impression"));
        }

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

        // Check GDPR: if regs.ext.gdpr == 1 and no user.ext.consent, warn (checked per bidder below)
        let gdpr_applies = bid_request.regs
            .as_ref()
            .and_then(|r| r.ext.as_ref())
            .and_then(|e| e.get("gdpr"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0) == 1;

        let has_consent = bid_request.user
            .as_ref()
            .and_then(|u| u.ext.as_ref())
            .and_then(|e| e.get("consent"))
            .and_then(|v| v.as_str())
            .map(|s| !s.is_empty())
            .unwrap_or(false);

        // Determine which bidders are active for this request by inspecting imp.ext.
        let active_bidders: Vec<String> = self
            .adapters
            .keys()
            .filter(|name| {
                bid_request.imp.iter().any(|imp| {
                    if let Some(ext) = &imp.ext {
                        // Support both `ext.<bidder>` and `ext.prebid.bidder.<bidder>` formats.
                        ext.get(*name).is_some()
                            || ext
                                .get("prebid")
                                .and_then(|p| p.get("bidder"))
                                .and_then(|b| b.as_object())
                                .map(|o| o.contains_key(*name))
                                .unwrap_or(false)
                    } else {
                        false
                    }
                })
            })
            .cloned()
            .collect();

        let extra_info = ExtraRequestInfo {
            pbs_entry_point: "openrtb2-auction".to_string(),
            global_privacy_control_header: String::new(),
        };

        let mut join_set = tokio::task::JoinSet::new();
        let mut collected_non_bids: Vec<SeatNonBid> = Vec::new();

        for bidder_name in active_bidders {
            // GDPR enforcement: skip bidder if GDPR applies but no consent string present
            if gdpr_applies && !has_consent {
                tracing::warn!(
                    bidder = %bidder_name,
                    "skipping bidder due to GDPR: gdpr=1 but no user.ext.consent string present"
                );
                // Record a SeatNonBid with reason code 50 (privacy) for each impression.
                let non_bids: Vec<NonBid> = bid_request.imp.iter().map(|imp| NonBid {
                    impid: imp.id.clone(),
                    statuscode: 50,
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

            if let Some(adapted) = self.adapters.get(&bidder_name) {
                // Use per-bidder timeout if configured, otherwise fall back to global.
                let effective_timeout_ms = per_bidder_timeouts
                    .get(&bidder_name)
                    .copied()
                    .unwrap_or(timeout_ms);

                // Clone everything needed for the async task.
                let adapted = AdaptedBidder {
                    bidder: adapted.bidder.clone(),
                    http_client: self.http_client.clone(),
                    endpoint: adapted.endpoint.clone(),
                    endpoint_compression: adapted.endpoint_compression.clone(),
                };
                let mut req = bid_request.clone();
                apply_fpd_for_bidder(&mut req, &bidder_name);
                let extra = extra_info.clone();
                let name = bidder_name.clone();

                join_set.spawn(async move {
                    adapted.request_bid(&req, &name, &extra, effective_timeout_ms).await
                });
            }
        }

        // --- Schain passthrough ---
        // If bid_request.source.schain is already set, it is included in the cloned BidRequest
        // sent to each bidder — no additional work is needed for the pass-through case.
        // (Host-configured schain node prepending would be added here when config supports it.)
        if let Some(source) = &bid_request.source {
            if source.schain.is_some() {
                tracing::debug!("schain present on request source; passing through to bidders as-is");
            }
        }

        // --- First-party data passthrough ---
        // FPD fields (site.ext.data, app.ext.data, user.ext.data, imp[].ext.data) are stored in
        // `ext: Option<serde_json::Value>` on the respective openrtb structs.  Each bidder receives
        // a full clone of BidRequest, so FPD passes through automatically.

        // Collect results: keep TypedBids per bidder so we can compute targeting before flattening.
        let mut bidder_results: Vec<(String, Vec<pbs_adapters::TypedBid>)> = Vec::new();
        let mut timed_out_bidders: Vec<String> = Vec::new();

        while let Some(result) = join_set.join_next().await {
            match result {
                Ok(bidder_result) => {
                    if bidder_result.timed_out {
                        timed_out_bidders.push(bidder_result.bidder_name.clone());
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

                                if !deduped.is_empty() {
                                    bidder_results.push((bidder_result.bidder_name, deduped));
                                }
                            }
                        }
                        Err(_errs) => {
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

        // Assemble seat bids from collected results, resolving ${AUCTION_PRICE} macros.
        let mut seat_bids: Vec<openrtb::SeatBid> = Vec::new();
        for (bidder_name, typed_bids) in bidder_results {
            let bids: Vec<openrtb::Bid> = typed_bids.into_iter().map(|tb| {
                let price = tb.bid.price;
                let mut bid = tb.bid;
                if let Some(nurl) = bid.nurl.as_deref() {
                    bid.nurl = Some(resolve_price_macro(nurl, price));
                }
                if let Some(adm) = bid.adm.as_deref() {
                    bid.adm = Some(resolve_price_macro(adm, price));
                }
                if let Some(burl) = bid.burl.as_deref() {
                    bid.burl = Some(resolve_price_macro(burl, price));
                }
                bid
            }).collect();
            seat_bids.push(openrtb::SeatBid {
                bid: bids,
                seat: Some(bidder_name),
                ..Default::default()
            });
        }

        let bid_response = openrtb::BidResponse {
            id: bid_request.id.clone(),
            seatbid: seat_bids,
            cur: Some("USD".to_string()),
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

        Ok(AuctionResponse {
            bid_response,
            seat_non_bids: collected_non_bids,
            targeting,
            timed_out_bidders,
        })
    }
}
