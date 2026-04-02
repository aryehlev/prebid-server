use std::collections::HashMap;
use std::sync::Arc;

use openrtb_ext::SeatNonBid;
use pbs_adapters::{BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData};

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
}

/// AuctionResponse is the output of Exchange::hold_auction.
pub struct AuctionResponse {
    pub bid_response: openrtb::BidResponse,
    pub seat_non_bids: Vec<SeatNonBid>,
}

/// BidderResult collects everything returned from a single bidder task.
pub struct BidderResult {
    pub bidder_name: String,
    pub response: Result<BidderResponse, Vec<BidderError>>,
    pub duration_ms: u64,
    pub http_calls: Vec<openrtb_ext::ExtHttpCall>,
    pub timed_out: bool,
}

/// AdaptedBidder wraps a Bidder implementation with HTTP execution logic.
pub struct AdaptedBidder {
    pub bidder: Arc<dyn pbs_adapters::Bidder>,
    pub http_client: reqwest::Client,
    pub endpoint: String,
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
            timed_out: false,
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

        builder = builder.body(req.body.clone());
        for (k, v) in &req.headers {
            builder = builder.header(k, v);
        }

        let req_body_str = String::from_utf8_lossy(&req.body).to_string();

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

/// Exchange orchestrates the full auction: fan-out to bidders, collect results, build response.
pub struct Exchange {
    pub adapters: HashMap<String, AdaptedBidder>,
    pub http_client: reqwest::Client,
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
        }
    }

    pub async fn hold_auction(
        &self,
        request: AuctionRequest,
    ) -> Result<AuctionResponse, anyhow::Error> {
        let bid_request = &request.bid_request;
        let tmax = bid_request.tmax.unwrap_or(5000) as u64;

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

        for bidder_name in active_bidders {
            if let Some(adapted) = self.adapters.get(&bidder_name) {
                // Clone everything needed for the async task.
                let adapted = AdaptedBidder {
                    bidder: adapted.bidder.clone(),
                    http_client: self.http_client.clone(),
                    endpoint: adapted.endpoint.clone(),
                };
                let req = bid_request.clone();
                let extra = extra_info.clone();
                let name = bidder_name.clone();

                join_set.spawn(async move {
                    adapted.request_bid(&req, &name, &extra, tmax).await
                });
            }
        }

        // Collect results and assemble seat bids.
        let mut seat_bids: Vec<openrtb::SeatBid> = Vec::new();

        while let Some(result) = join_set.join_next().await {
            match result {
                Ok(bidder_result) => {
                    if let Ok(response) = bidder_result.response {
                        if !response.bids.is_empty() {
                            let bids: Vec<openrtb::Bid> =
                                response.bids.into_iter().map(|tb| tb.bid).collect();
                            seat_bids.push(openrtb::SeatBid {
                                bid: bids,
                                seat: Some(bidder_result.bidder_name),
                                ..Default::default()
                            });
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Bidder task panicked: {}", e);
                }
            }
        }

        let bid_response = openrtb::BidResponse {
            id: bid_request.id.clone(),
            seatbid: seat_bids,
            cur: Some("USD".to_string()),
            ..Default::default()
        };

        Ok(AuctionResponse {
            bid_response,
            seat_non_bids: Vec::new(),
        })
    }
}
