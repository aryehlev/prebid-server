use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_bid_type_from_mtype, get_imp_ids};
use openrtb_ext::BidType;

pub struct SilverpushAdapter { pub endpoint: String }
impl SilverpushAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

fn get_media_type_for_imp(mtype: i32) -> Option<BidType> {
    match mtype {
        1 => Some(BidType::Banner),
        2 => Some(BidType::Video),
        _ => None,
    }
}

impl Bidder for SilverpushAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();
        let mut errs = Vec::new();
        for imp in &request.imp {
            let mut req_copy = request.clone();
            req_copy.imp = vec![imp.clone()];
            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };
            let mut headers = HashMap::new();
            headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
            headers.insert("Accept".to_string(), "application/json".to_string());
            headers.insert("X-Openrtb-Version".to_string(), "2.5".to_string());
            requests.push(RequestData { method: "POST".to_string(), uri: self.endpoint.clone(), body, headers, imp_ids: vec![imp.id.clone()] });
        }
        (requests, errs)
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(5);
        if let Some(cur) = &bid_resp.cur { if !cur.is_empty() { result.currency = cur.clone(); } }
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let mtype = bid.mtype.unwrap_or(0);
                if let Some(bid_type) = get_media_type_for_imp(mtype) {
                    result.bids.push(TypedBid::new(bid, bid_type));
                }
                // else: skip bids with unknown mtype (silverpush returns "" for unknown which is skipped)
            }
        }
        Ok(result)
    }
}
