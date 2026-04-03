use std::collections::HashMap;
use std::sync::Arc;

use openrtb_ext::{NonBid, SeatNonBid};
use pbs_adapters::{BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData};

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
                    errs.push(BidderError::Timeout);
                }
                Err(e) => errs.push(e),
            }
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

/// price_granularity_bucket returns the "medium" granularity price bucket for a bid price.
/// Medium granularity: $0.01 increments capped at $20.00.
fn price_granularity_bucket(price: f64) -> String {
    let capped = price.min(20.0);
    let bucket = (capped * 100.0).floor() / 100.0;
    format!("{:.2}", bucket)
}

/// Exchange orchestrates the full auction: fan-out to bidders, collect results, build response.
pub struct Exchange {
    pub adapters: HashMap<String, AdaptedBidder>,
    pub http_client: reqwest::Client,
    pub metrics: Option<Arc<dyn pbs_metrics::MetricsEngine>>,
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
        }
    }

    pub async fn hold_auction(
        &self,
        request: AuctionRequest,
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

        // Determine dynamic timeout from tmax
        let timeout_ms = bid_request.tmax
            .filter(|&t| t > 0)
            .unwrap_or(1000) as u64;
        let _duration = std::time::Duration::from_millis(timeout_ms);

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
                    adapted.request_bid(&req, &name, &extra, timeout_ms).await
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

        while let Some(result) = join_set.join_next().await {
            match result {
                Ok(bidder_result) => {
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

                                if !accepted.is_empty() {
                                    bidder_results.push((bidder_result.bidder_name, accepted));
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

        // Per-bidder keys.
        for (bidder_name, typed_bids) in &bidder_results {
            for typed_bid in typed_bids {
                let keys = targeting.entry(typed_bid.bid.impid.clone()).or_default();
                keys.insert(
                    format!("hb_pb_{}", bidder_name),
                    price_granularity_bucket(typed_bid.bid.price),
                );
                keys.insert(
                    format!("hb_bidder_{}", bidder_name),
                    bidder_name.clone(),
                );
                keys.insert(
                    format!("hb_adid_{}", bidder_name),
                    typed_bid.bid.id.clone(),
                );
            }
        }

        // Winner keys: highest bid per impression.
        let mut winners: HashMap<String, (f64, String, String)> = HashMap::new();
        for (bidder_name, typed_bids) in &bidder_results {
            for typed_bid in typed_bids {
                let entry = winners
                    .entry(typed_bid.bid.impid.clone())
                    .or_insert((f64::NEG_INFINITY, String::new(), String::new()));
                if typed_bid.bid.price > entry.0 {
                    *entry = (
                        typed_bid.bid.price,
                        bidder_name.clone(),
                        typed_bid.bid.id.clone(),
                    );
                }
            }
        }
        for (imp_id, (price, bidder_name, bid_id)) in &winners {
            let keys = targeting.entry(imp_id.clone()).or_default();
            keys.insert("hb_pb".to_string(), price_granularity_bucket(*price));
            keys.insert("hb_bidder".to_string(), bidder_name.clone());
            keys.insert("hb_adid".to_string(), bid_id.clone());
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

        Ok(AuctionResponse {
            bid_response,
            seat_non_bids: collected_non_bids,
            targeting,
        })
    }
}
