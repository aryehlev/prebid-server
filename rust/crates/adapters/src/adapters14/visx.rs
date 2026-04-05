use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct VisxAdapter { pub endpoint: String }
impl VisxAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

fn get_bid_type(bid: &openrtb::Bid, imp: Option<&openrtb::Imp>) -> Result<BidType, BidderError> {
    // Try ext.prebid.meta.mediaType first
    let meta_type = bid.ext.as_ref()
        .and_then(|e| e.get("prebid"))
        .and_then(|p| p.get("meta"))
        .and_then(|m| m.get("mediaType"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if meta_type == "banner" { return Ok(BidType::Banner); }
    if meta_type == "video" { return Ok(BidType::Video); }
    // Fall back to imp-based
    if let Some(imp) = imp {
        if imp.banner.is_some() { return Ok(BidType::Banner); }
        if imp.video.is_some() { return Ok(BidType::Video); }
        return Err(BidderError::BadServerResponse(format!("Unknown impression type for ID: \"{}\"", bid.impid)));
    }
    Err(BidderError::BadServerResponse(format!("Failed to find impression for ID: \"{}\"", bid.impid)))
}

impl Bidder for VisxAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut req_copy = request.clone();
        if req_copy.cur.is_none() || req_copy.cur.as_ref().map(|c| c.is_empty()).unwrap_or(true) {
            req_copy.cur = Some(vec!["USD".to_string()]);
        }
        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        if let Some(device) = &request.device {
            if let Some(ip) = &device.ip { if !ip.is_empty() { headers.insert("X-Forwarded-For".to_string(), ip.clone()); } }
            if let Some(ip6) = &device.ipv6 { if !ip6.is_empty() { headers.insert("X-Forwarded-For".to_string(), ip6.clone()); } }
        }
        (vec![RequestData { method: "POST".to_string(), uri: self.endpoint.clone(), body, headers, imp_ids: get_imp_ids(&request.imp) }], vec![])
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code == 400 {
            return Err(vec![BidderError::BadInput(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info",
                response.status_code
            ))]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info",
                response.status_code
            ))]);
        }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(1);
        if let Some(cur) = &bid_resp.cur { if !cur.is_empty() { result.currency = cur.clone(); } }
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let imp = internal.imp.iter().find(|i| i.id == bid.impid);
                match get_bid_type(&bid, imp) {
                    Ok(t) => result.bids.push(TypedBid::new(bid, t)),
                    Err(e) => return Err(vec![e]),
                }
            }
        }
        Ok(result)
    }
}
