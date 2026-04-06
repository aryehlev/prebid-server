use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::{BidType, ExtBidPrebidMeta, FledgeAuctionConfig};
use serde::Deserialize;
use serde_json::Value;

pub struct CriteoAdapter {
    pub endpoint: String,
    pub bidder_name: String,
}

impl CriteoAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint, bidder_name: "criteo".to_string() }
    }

    pub fn new_with_name(endpoint: String, bidder_name: String) -> Self {
        Self { endpoint, bidder_name }
    }
}

#[derive(Deserialize)]
struct BidExt {
    prebid: ExtPrebid,
}

#[derive(Deserialize)]
struct ExtPrebid {
    #[serde(rename = "type")]
    bid_type: Option<String>,
    #[serde(rename = "networkName", default)]
    network_name: String,
}

#[derive(Deserialize)]
struct CriteoExt {
    #[serde(default)]
    igi: Vec<CriteoExtIgi>,
}

#[derive(Deserialize)]
struct CriteoExtIgi {
    impid: String,
    #[serde(default)]
    igs: Vec<CriteoExtIgs>,
}

#[derive(Deserialize)]
struct CriteoExtIgs {
    config: Option<Value>,
}

fn parse_bid_type(s: &str) -> BidType {
    match s {
        "banner" => BidType::Banner,
        "video" => BidType::Video,
        "native" => BidType::Native,
        "audio" => BidType::Audio,
        _ => BidType::Banner,
    }
}

impl Bidder for CriteoAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let body = match serde_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());

        (
            vec![RequestData {
                method: "POST".to_string(),
                uri: self.endpoint.clone(),
                body,
                headers,
                imp_ids: get_imp_ids(&request.imp),
            }],
            vec![],
        )
    }

    fn make_bids(
        &self,
        _internal: &openrtb::BidRequest,
        _external: &RequestData,
        response: &ResponseData,
    ) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 {
            return Ok(BidderResponse::new());
        }
        if response.status_code == 400 {
            return Err(vec![BidderError::BadInput(
                "Unexpected status code: 400. Run with request.debug = 1 for more info.".to_string(),
            )]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info.",
                response.status_code
            ))]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::new();
        if let Some(cur) = &bid_resp.cur {
            if !cur.is_empty() {
                result.currency = cur.clone();
            }
        }

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_ext: BidExt = match bid.ext.as_ref()
                    .and_then(|e| serde_json::from_value::<BidExt>(e.clone()).ok())
                {
                    Some(e) => e,
                    None => return Err(vec![BidderError::BadServerResponse(format!(
                        "Missing ext.prebid.type in bid for impression : {}.",
                        bid.impid
                    ))]),
                };

                let bid_type = match bid_ext.prebid.bid_type.as_deref() {
                    Some(t) => parse_bid_type(t),
                    None => return Err(vec![BidderError::BadServerResponse(format!(
                        "Missing ext.prebid.type in bid for impression : {}.",
                        bid.impid
                    ))]),
                };

                let bid_meta = if !bid_ext.prebid.network_name.is_empty() {
                    Some(ExtBidPrebidMeta {
                        network_name: Some(bid_ext.prebid.network_name.clone()),
                        ..Default::default()
                    })
                } else {
                    None
                };

                let mut typed_bid = TypedBid::new(bid, bid_type);
                typed_bid.bid_meta = bid_meta;
                result.bids.push(typed_bid);
            }
        }

        // Parse FLEDGE auction configs from response ext
        if let Some(ext) = &bid_resp.ext {
            if let Ok(criteo_ext) = serde_json::from_value::<CriteoExt>(ext.clone()) {
                for igi in criteo_ext.igi {
                    if let Some(first_igs) = igi.igs.first() {
                        if let Some(config) = &first_igs.config {
                            result.fledge_auction_configs.push(FledgeAuctionConfig {
                                impid: igi.impid.clone(),
                                bidder: Some(self.bidder_name.clone()),
                                adapter: None,
                                config: config.clone(),
                            });
                        }
                    }
                }
            }
        }

        Ok(result)
    }
}
