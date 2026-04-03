use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_bid_type_from_mtype, get_imp_ids};

pub struct RiseAdapter { pub endpoint: String }
impl RiseAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

fn extract_org(request: &openrtb::BidRequest) -> Result<String, BidderError> {
    for imp in &request.imp {
        let bidder = imp.ext.as_ref().and_then(|e| e.get("bidder"));
        let org = bidder.and_then(|b| b.get("org")).and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
        if !org.is_empty() { return Ok(org); }
        let pub_id = bidder.and_then(|b| b.get("publisher_id")).and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
        if !pub_id.is_empty() { return Ok(pub_id); }
    }
    Err(BidderError::BadInput("no org or publisher_id supplied".to_string()))
}

impl Bidder for RiseAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let org = match extract_org(request) {
            Ok(o) => o,
            Err(e) => return (vec![], vec![e]),
        };
        let body = match serde_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        let uri = format!("{}?publisher_id={}", self.endpoint, org);
        (vec![RequestData { method: "POST".to_string(), uri, body, headers, imp_ids: get_imp_ids(&request.imp) }], vec![])
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(5);
        if let Some(cur) = &bid_resp.cur { if !cur.is_empty() { result.currency = cur.clone(); } }
        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let mtype = bid.mtype.unwrap_or(0);
                if mtype == 0 {
                    errs.push(BidderError::BadServerResponse(format!("unsupported MType {}", mtype)));
                    continue;
                }
                result.bids.push(TypedBid::new(bid, get_bid_type_from_mtype(mtype)));
            }
        }
        if !errs.is_empty() && result.bids.is_empty() { return Err(errs); }
        Ok(result)
    }
}
