use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::{BidType, ExtBidPrebidMeta, ExtBidPrebidVideo};
use serde::Deserialize;
use serde_json::Value;

pub struct AdagioAdapter { pub endpoint: String }
impl AdagioAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

#[derive(Deserialize, Default)]
struct ExtBidPrebidAdagio {
    #[serde(default)]
    meta: Option<ExtBidPrebidMeta>,
    #[serde(default)]
    video: Option<ExtBidPrebidVideo>,
}

#[derive(Deserialize, Default)]
struct ExtBidAdagio {
    prebid: Option<ExtBidPrebidAdagio>,
}

fn get_bid_type(mtype: Option<i32>) -> Result<BidType, BidderError> {
    match mtype {
        Some(1) => Ok(BidType::Banner),
        Some(2) => Ok(BidType::Video),
        Some(4) => Ok(BidType::Native),
        _ => Err(BidderError::BadInput(format!(
            "Could not define media type for impression"
        ))),
    }
}

fn get_bid_ext(ext: &Option<Value>) -> Option<ExtBidAdagio> {
    ext.as_ref().and_then(|e| serde_json::from_value(e.clone()).ok())
}

impl Bidder for AdagioAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let body = match serde_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
        (vec![RequestData { method: "POST".to_string(), uri: self.endpoint.clone(), body, headers, imp_ids: get_imp_ids(&request.imp) }], vec![])
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(
                format!("bid response, err: {}", e)
            )])?;
        if bid_resp.seatbid.is_empty() {
            return Err(vec![BidderError::BadServerResponse("empty seatbid array".to_string())]);
        }
        let mut result = BidderResponse::with_capacity(5);
        if let Some(cur) = &bid_resp.cur { result.currency = cur.clone(); }
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = match get_bid_type(bid.mtype) {
                    Ok(bt) => bt,
                    Err(_) => continue,
                };
                let bid_ext = get_bid_ext(&bid.ext);
                let (bid_meta, bid_video) = if let Some(ext) = bid_ext {
                    if let Some(prebid) = ext.prebid {
                        (prebid.meta, prebid.video)
                    } else {
                        (None, None)
                    }
                } else {
                    (None, None)
                };
                let mut typed_bid = TypedBid::new(bid, bid_type);
                typed_bid.bid_meta = bid_meta;
                typed_bid.bid_video = bid_video;
                result.bids.push(typed_bid);
            }
        }
        Ok(result)
    }
}
