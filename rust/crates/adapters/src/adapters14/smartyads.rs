use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_bid_type_from_imp, get_imp_ids};

pub struct SmartyadsAdapter { pub endpoint: String }
impl SmartyadsAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

fn get_headers(request: &openrtb::BidRequest) -> HashMap<String, String> {
    let mut h = HashMap::new();
    h.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
    h.insert("Accept".to_string(), "application/json".to_string());
    h.insert("X-Openrtb-Version".to_string(), "2.5".to_string());
    if let Some(device) = &request.device {
        if let Some(ua) = &device.ua { h.insert("User-Agent".to_string(), ua.clone()); }
        if let Some(ip) = &device.ip { h.insert("X-Forwarded-For".to_string(), ip.clone()); }
    }
    h
}

impl Bidder for SmartyadsAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::BadInput("no impressions".to_string())]);
        }
        let bidder = request.imp[0].ext.as_ref().and_then(|e| e.get("bidder"));
        let host = bidder.and_then(|b| b.get("host")).and_then(|v| v.as_str()).unwrap_or("");
        let source_id = bidder.and_then(|b| b.get("sourceId")).and_then(|v| v.as_str()).unwrap_or("");
        let account_id = bidder.and_then(|b| b.get("accountID")).and_then(|v| v.as_str()).unwrap_or("");
        let url = self.endpoint
            .replace("{{.Host}}", host)
            .replace("{{.SourceId}}", source_id)
            .replace("{{.AccountID}}", account_id);
        let mut req_copy = request.clone();
        for imp in &mut req_copy.imp {
            imp.ext = None;
        }
        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };
        let headers = get_headers(request);
        (vec![RequestData { method: "POST".to_string(), uri: url, body, headers, imp_ids: get_imp_ids(&request.imp) }], vec![])
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(5);
        if let Some(cur) = &bid_resp.cur { if !cur.is_empty() { result.currency = cur.clone(); } }
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = internal.imp.iter().find(|i| i.id == bid.impid).map(get_bid_type_from_imp).unwrap_or(openrtb_ext::BidType::Banner);
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        Ok(result)
    }
}
