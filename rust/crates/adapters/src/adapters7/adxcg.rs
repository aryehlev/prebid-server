use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct AdxcgAdapter { pub endpoint: String }
impl AdxcgAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

fn mtype_to_bid_type(mtype: Option<i32>) -> Result<BidType, BidderError> {
    match mtype {
        Some(1) => Ok(BidType::Banner),
        Some(2) => Ok(BidType::Video),
        Some(4) => Ok(BidType::Native),
        Some(3) => Ok(BidType::Audio),
        _ => Err(BidderError::BadServerResponse("Unsupported return type".to_string())),
    }
}

impl Bidder for AdxcgAdapter {
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
                match mtype_to_bid_type(bid.mtype) {
                    Ok(bt) => result.bids.push(TypedBid::new(bid, bt)),
                    Err(e) => errs.push(e),
                }
            }
        }
        Ok(result)
    }
}
