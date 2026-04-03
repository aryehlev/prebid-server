use std::collections::HashMap;

use crate::{
    Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid,
    get_imp_ids,
};
use openrtb_ext::{BidType, ExtBidPrebidMeta, FledgeAuctionConfig};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub struct CriteoAdapter {
    pub endpoint: String,
    pub bidder_name: String,
}

impl CriteoAdapter {
    pub fn new(endpoint: String, bidder_name: String) -> Self {
        Self {
            endpoint,
            bidder_name,
        }
    }
}

/// Criteo bid ext for bid type and network name
#[derive(Debug, Default, Deserialize)]
struct CriteoBidExt {
    prebid: CriteoBidExtPrebid,
}

#[derive(Debug, Default, Deserialize)]
struct CriteoBidExtPrebid {
    #[serde(rename = "type", default)]
    bid_type: String,
    #[serde(rename = "networkName", default)]
    network_name: String,
}

/// Criteo response ext for FLEDGE/IGI
#[derive(Debug, Default, Deserialize)]
struct CriteoRespExt {
    #[serde(default)]
    igi: Vec<CriteoExtIgi>,
}

#[derive(Debug, Default, Deserialize)]
struct CriteoExtIgi {
    #[serde(default)]
    impid: String,
    #[serde(default)]
    igs: Vec<CriteoExtIgs>,
}

#[derive(Debug, Default, Deserialize)]
struct CriteoExtIgs {
    config: Option<Value>,
}

fn parse_bid_type(s: &str) -> Option<BidType> {
    match s {
        "banner" => Some(BidType::Banner),
        "video" => Some(BidType::Video),
        "native" => Some(BidType::Native),
        "audio" => Some(BidType::Audio),
        _ => None,
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

        let imp_ids = get_imp_ids(&request.imp);

        let req = RequestData {
            method: "POST".to_string(),
            uri: self.endpoint.clone(),
            body,
            headers: HashMap::new(),
            imp_ids,
        };

        (vec![req], vec![])
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

        let bid_response: openrtb::BidResponse =
            serde_json::from_slice(&response.body)
                .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::new();
        if let Some(cur) = &bid_response.cur {
            result.currency = cur.clone();
        }

        for seat_bid in &bid_response.seatbid {
            for bid in &seat_bid.bid {
                let bid_ext: CriteoBidExt = bid.ext.as_ref()
                    .and_then(|e| serde_json::from_value(e.clone()).ok())
                    .ok_or_else(|| vec![BidderError::BadServerResponse(format!(
                        "Missing ext.prebid.type in bid for impression : {}.", bid.impid
                    ))])?;

                let bid_type = parse_bid_type(&bid_ext.prebid.bid_type)
                    .ok_or_else(|| vec![BidderError::BadServerResponse(format!(
                        "Missing ext.prebid.type in bid for impression : {}.", bid.impid
                    ))])?;

                let bid_meta = if !bid_ext.prebid.network_name.is_empty() {
                    Some(ExtBidPrebidMeta {
                        network_name: Some(bid_ext.prebid.network_name),
                        ..Default::default()
                    })
                } else {
                    None
                };

                let mut typed_bid = TypedBid::new(bid.clone(), bid_type);
                typed_bid.bid_meta = bid_meta;
                result.bids.push(typed_bid);
            }
        }

        // Parse FLEDGE auction configs from response ext IGI
        if let Some(ext) = &bid_response.ext {
            if let Ok(resp_ext) = serde_json::from_value::<CriteoRespExt>(ext.clone()) {
                let mut fledge_configs = Vec::new();
                for igi in resp_ext.igi {
                    if let Some(igs) = igi.igs.first() {
                        if let Some(cfg) = &igs.config {
                            fledge_configs.push(FledgeAuctionConfig {
                                impid: igi.impid,
                                bidder: Some(self.bidder_name.clone()),
                                adapter: None,
                                config: cfg.clone(),
                            });
                        }
                    }
                }
                if !fledge_configs.is_empty() {
                    result.fledge_auction_configs = fledge_configs;
                }
            }
        }

        Ok(result)
    }
}
