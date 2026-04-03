use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct VideobyteAdapter { pub endpoint: String }
impl VideobyteAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

fn get_media_type_for_imp(imp: &openrtb::Imp) -> BidType {
    if imp.banner.is_some() { BidType::Banner } else { BidType::Video }
}

fn get_headers(request: &openrtb::BidRequest) -> HashMap<String, String> {
    let mut h = HashMap::new();
    h.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
    h.insert("Accept".to_string(), "application/json".to_string());
    if let Some(device) = &request.device {
        if let Some(ua) = &device.ua { h.insert("User-Agent".to_string(), ua.clone()); }
        if let Some(ip) = &device.ip { h.insert("X-Forwarded-For".to_string(), ip.clone()); }
    }
    h
}

impl Bidder for VideobyteAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();
        let mut errs = Vec::new();
        let original_imps = request.imp.clone();
        let headers = get_headers(request);
        for imp in &original_imps {
            let bidder = imp.ext.as_ref().and_then(|e| e.get("bidder"));
            let publisher_id = bidder.and_then(|b| b.get("publisherId")).and_then(|v| v.as_str()).unwrap_or("");
            let placement_id = bidder.and_then(|b| b.get("placementId")).and_then(|v| v.as_str()).unwrap_or("");
            let network_id = bidder.and_then(|b| b.get("networkId")).and_then(|v| v.as_str()).unwrap_or("");
            let mut params = vec![("source".to_string(), "pbs".to_string()), ("pid".to_string(), publisher_id.to_string())];
            if !placement_id.is_empty() { params.push(("placementId".to_string(), placement_id.to_string())); }
            if !network_id.is_empty() { params.push(("nid".to_string(), network_id.to_string())); }
            let query: String = params.iter().map(|(k, v)| format!("{}={}", k, v)).collect::<Vec<_>>().join("&");
            let uri = format!("{}?{}", self.endpoint, query);
            let mut req_copy = request.clone();
            req_copy.imp = vec![imp.clone()];
            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };
            requests.push(RequestData { method: "POST".to_string(), uri, body, headers: headers.clone(), imp_ids: vec![imp.id.clone()] });
        }
        (requests, errs)
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(5);
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = internal.imp.iter().find(|i| i.id == bid.impid)
                    .map(get_media_type_for_imp).unwrap_or(BidType::Video);
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        Ok(result)
    }
}
