use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_bid_type_from_mtype, get_imp_ids};

pub struct SmrtconnectAdapter { pub endpoint: String }
impl SmrtconnectAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

impl Bidder for SmrtconnectAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        // per-imp requests with template URL resolution
        let mut requests = Vec::new();
        let mut errs = Vec::new();
        for imp in &request.imp {
            let host = imp.ext.as_ref()
                .and_then(|e| e.get("bidder"))
                .and_then(|b| b.get("host"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let source_id = imp.ext.as_ref()
                .and_then(|e| e.get("bidder"))
                .and_then(|b| b.get("sourceId"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let url = self.endpoint
                .replace("{{.Host}}", &host)
                .replace("{{.SourceId}}", &source_id);
            let mut req_copy = request.clone();
            req_copy.imp = vec![imp.clone()];
            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };
            let mut headers = HashMap::new();
            headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
            requests.push(RequestData { method: "POST".to_string(), uri: url, body, headers, imp_ids: vec![imp.id.clone()] });
        }
        (requests, errs)
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(5);
        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let mtype = bid.mtype.unwrap_or(0);
                if mtype == 0 {
                    errs.push(BidderError::BadInput(format!("Could not define media type for impression: {}", bid.impid)));
                    continue;
                }
                result.bids.push(TypedBid::new(bid, get_bid_type_from_mtype(mtype)));
            }
        }
        if !errs.is_empty() && result.bids.is_empty() { return Err(errs); }
        Ok(result)
    }
}
