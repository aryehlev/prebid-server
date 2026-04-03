use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_bid_type_from_mtype, get_imp_ids};
use serde_json::json;

pub struct SmootAdapter { pub endpoint: String }
impl SmootAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

impl Bidder for SmootAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();
        let mut errs = Vec::new();
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        for imp in &request.imp {
            let bidder = imp.ext.as_ref().and_then(|e| e.get("bidder"));
            let placement_id = bidder.and_then(|b| b.get("placementId")).and_then(|v| v.as_str()).unwrap_or("").to_string();
            let endpoint_id = bidder.and_then(|b| b.get("endpointId")).and_then(|v| v.as_str()).unwrap_or("").to_string();

            let imp_ext = if !placement_id.is_empty() {
                json!({ "bidder": { "type": "publisher", "placementId": placement_id } })
            } else {
                json!({ "bidder": { "type": "network", "endpointId": endpoint_id } })
            };

            let mut imp_copy = imp.clone();
            imp_copy.ext = Some(imp_ext);
            let mut req_copy = request.clone();
            req_copy.imp = vec![imp_copy];

            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };
            requests.push(RequestData { method: "POST".to_string(), uri: self.endpoint.clone(), body, headers: headers.clone(), imp_ids: vec![imp.id.clone()] });
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
        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let mtype = bid.mtype.unwrap_or(0);
                if mtype == 0 {
                    errs.push(BidderError::BadServerResponse(format!("could not define media type for impression: {}", bid.impid)));
                    continue;
                }
                result.bids.push(TypedBid::new(bid, get_bid_type_from_mtype(mtype)));
            }
        }
        if !errs.is_empty() && result.bids.is_empty() { return Err(errs); }
        Ok(result)
    }
}
