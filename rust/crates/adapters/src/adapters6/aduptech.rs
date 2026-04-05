use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct AduptechAdapter { pub endpoint: String }
impl AduptechAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

/// Maps OpenRTB mtype to BidType for aduptech.
/// Only Banner (1) and Native (4) are supported; all others return an error.
fn get_bid_type(mtype: Option<i32>) -> Result<BidType, BidderError> {
    match mtype {
        Some(4) => Ok(BidType::Native),
        Some(1) => Ok(BidType::Banner),
        other => Err(BidderError::BadServerResponse(format!("Unknown markup type: {}", other.unwrap_or(0)))),
    }
}

impl Bidder for AduptechAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let body = match serde_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        (vec![RequestData { method: "POST".to_string(), uri: self.endpoint.clone(), body, headers, imp_ids: get_imp_ids(&request.imp) }], vec![])
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(5);
        if let Some(cur) = &bid_resp.cur { result.currency = cur.clone(); }
        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                match get_bid_type(bid.mtype) {
                    Ok(bt) => result.bids.push(TypedBid::new(bid, bt)),
                    Err(e) => errs.push(e),
                }
            }
        }
        Ok(result)
    }
}
