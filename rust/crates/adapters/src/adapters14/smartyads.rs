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
        if let Some(ua) = &device.ua {
            if !ua.is_empty() { h.insert("User-Agent".to_string(), ua.clone()); }
        }
        // Prefer IPv6 first, then IPv4 (matching Go behaviour: Add both)
        if let Some(ipv6) = &device.ipv6 {
            if !ipv6.is_empty() { h.insert("X-Forwarded-For".to_string(), ipv6.clone()); }
        }
        if let Some(ip) = &device.ip {
            if !ip.is_empty() { h.insert("X-Forwarded-For".to_string(), ip.clone()); }
        }
        if let Some(lang) = &device.language {
            if !lang.is_empty() { h.insert("Accept-Language".to_string(), lang.clone()); }
        }
        if let Some(dnt) = &device.dnt {
            h.insert("Dnt".to_string(), dnt.to_string());
        }
    }
    h
}

impl Bidder for SmartyadsAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::BadInput("no impressions".to_string())]);
        }

        // Extract host, sourceid, accountid from the last imp's bidder ext (matching Go: loop overwrites)
        let bidder = request.imp.last().and_then(|imp| imp.ext.as_ref()).and_then(|e| e.get("bidder"));
        let host = bidder.and_then(|b| b.get("host")).and_then(|v| v.as_str()).unwrap_or("");
        let source_id = bidder.and_then(|b| b.get("sourceid")).and_then(|v| v.as_str()).unwrap_or("");
        let account_id = bidder.and_then(|b| b.get("accountid")).and_then(|v| v.as_str()).unwrap_or("");

        let url = self.endpoint
            .replace("{{.Host}}", host)
            .replace("{{.SourceId}}", source_id)
            .replace("{{.AccountID}}", account_id);

        // Strip bidder ext from all imps before sending
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
        // 204 No Content treated as BadInput (no bid response), matching Go behaviour
        if response.status_code == 204 {
            return Err(vec![BidderError::BadInput("No bid response".to_string())]);
        }
        if response.status_code == 400 {
            return Err(vec![BidderError::BadInput(format!(
                "Unexpected status code: [ {} ]", response.status_code
            ))]);
        }
        if response.status_code == 503 || (response.status_code < 200 || response.status_code >= 300) {
            return Err(vec![BidderError::BadInput(format!(
                "Something went wrong, please contact your Account Manager. Status Code: [ {} ] ",
                response.status_code
            ))]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|_| vec![BidderError::BadServerResponse("Bad Server Response".to_string())])?;

        if bid_resp.seatbid.is_empty() {
            return Err(vec![BidderError::BadServerResponse("Empty SeatBid array".to_string())]);
        }

        let mut result = BidderResponse::with_capacity(bid_resp.seatbid[0].bid.len());
        for bid in &bid_resp.seatbid[0].bid {
            let bid_type = internal.imp.iter().find(|i| i.id == bid.impid)
                .map(get_bid_type_from_imp)
                .unwrap_or(openrtb_ext::BidType::Banner);
            result.bids.push(TypedBid::new(bid.clone(), bid_type));
        }
        Ok(result)
    }
}
