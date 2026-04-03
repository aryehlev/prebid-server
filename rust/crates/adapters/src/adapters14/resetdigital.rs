use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_bid_type_from_mtype, get_bid_type_from_imp, get_imp_ids};

pub struct ResetdigitalAdapter { pub endpoint: String }
impl ResetdigitalAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

impl Bidder for ResetdigitalAdapter {
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

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(5);
        if let Some(cur) = &bid_resp.cur { if !cur.is_empty() { result.currency = cur.clone(); } }
        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = if bid.mtype.map(|m| m > 0).unwrap_or(false) {
                    get_bid_type_from_mtype(bid.mtype.unwrap())
                } else {
                    // fall back to imp-based
                    if let Some(imp) = internal.imp.iter().find(|i| i.id == bid.impid) {
                        get_bid_type_from_imp(imp)
                    } else {
                        errs.push(BidderError::BadServerResponse(format!("no matching impression found for ImpID: {}", bid.impid)));
                        continue;
                    }
                };
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        if !errs.is_empty() && result.bids.is_empty() { return Err(errs); }
        Ok(result)
    }
}
