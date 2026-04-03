use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct VidoomyAdapter { pub endpoint: String }
impl VidoomyAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

fn get_imp_info(imp_id: &str, imps: &[openrtb::Imp]) -> Option<BidType> {
    for imp in imps {
        if imp.id == imp_id {
            if imp.video.is_some() { return Some(BidType::Video); }
            if imp.banner.is_some() { return Some(BidType::Banner); }
        }
    }
    None
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

impl Bidder for VidoomyAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();
        let mut errs = Vec::new();
        let headers = get_headers(request);
        for imp in &request.imp {
            let mut req_copy = request.clone();
            req_copy.imp = vec![imp.clone()];
            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };
            requests.push(RequestData { method: "POST".to_string(), uri: self.endpoint.clone(), body, headers: headers.clone(), imp_ids: vec![imp.id.clone()] });
        }
        (requests, errs)
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(5);
        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                if let Some(bid_type) = get_imp_info(&bid.impid, &internal.imp) {
                    if bid_type == BidType::Banner || bid_type == BidType::Video {
                        result.bids.push(TypedBid::new(bid, bid_type));
                    } else {
                        errs.push(BidderError::BadServerResponse("unsupported bid type".to_string()));
                    }
                } else {
                    errs.push(BidderError::BadServerResponse(format!("no imp found for {}", bid.impid)));
                }
            }
        }
        if !errs.is_empty() && result.bids.is_empty() { return Err(errs); }
        Ok(result)
    }
}
