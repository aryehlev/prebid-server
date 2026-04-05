use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::{BidType, ExtBidPrebidVideo};
use serde::Deserialize;

pub struct AvocetAdapter {
    pub endpoint: String,
}

impl AvocetAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

#[derive(Deserialize, Default)]
struct AvocetBidExtension {
    #[serde(default)]
    duration: i32,
    #[serde(default)]
    deal_priority: i32,
}

#[derive(Deserialize, Default)]
struct AvocetBidExt {
    #[serde(default)]
    avocet: AvocetBidExtension,
}

fn get_bid_type(bid: &openrtb::Bid, ext: &AvocetBidExtension) -> BidType {
    if ext.duration != 0 {
        return BidType::Video;
    }
    // bid.api: VPAID 1.0 = 1, VPAID 2.0 = 2
    match bid.api {
        Some(1) | Some(2) => BidType::Video,
        _ => BidType::Banner,
    }
}

impl Bidder for AvocetAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![]);
        }
        let body = match serde_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
        (vec![RequestData {
            method: "POST".to_string(),
            uri: self.endpoint.clone(),
            body,
            headers,
            imp_ids: get_imp_ids(&request.imp),
        }], vec![])
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code != 200 {
            let body_str = if response.body.is_empty() {
                "no response body".to_string()
            } else {
                String::from_utf8_lossy(&response.body).to_string()
            };
            return Err(vec![BidderError::BadServerResponse(format!(
                "received status code: {} error: {}", response.status_code, body_str
            ))]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(5);
        let mut errs = Vec::new();

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let avocet_ext: AvocetBidExt = if let Some(ext) = &bid.ext {
                    match serde_json::from_value(ext.clone()) {
                        Ok(e) => e,
                        Err(e) => {
                            errs.push(BidderError::BadServerResponse(e.to_string()));
                            continue;
                        }
                    }
                } else {
                    AvocetBidExt::default()
                };

                let bid_type = get_bid_type(&bid, &avocet_ext.avocet);
                let mut typed_bid = TypedBid::new(bid, bid_type.clone());
                typed_bid.deal_priority = avocet_ext.avocet.deal_priority;
                if bid_type == BidType::Video {
                    typed_bid.bid_video = Some(ExtBidPrebidVideo {
                        duration: avocet_ext.avocet.duration,
                        primary_category: String::new(),
                    });
                }
                result.bids.push(typed_bid);
            }
        }

        if !errs.is_empty() {
            return Err(errs);
        }
        Ok(result)
    }
}
