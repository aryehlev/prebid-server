use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct FeedadAdapter { pub endpoint: String }
impl FeedadAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

impl Bidder for FeedadAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let body = match serde_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };
        let mut headers = HashMap::new();
        headers.insert("Accept".to_string(), "application/json".to_string());
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("X-FA-PBS-Adapter-Version".to_string(), "1.0.0".to_string());
        headers.insert("X-Openrtb-Version".to_string(), "2.5".to_string());
        if let Some(device) = &request.device {
            // IPv6 first, then IPv4 (both added; last write wins in HashMap, so IPv4 wins)
            if let Some(ip6) = &device.ipv6 { if !ip6.is_empty() { headers.insert("X-Forwarded-For".to_string(), ip6.clone()); } }
            if let Some(ip) = &device.ip { if !ip.is_empty() { headers.insert("X-Forwarded-For".to_string(), ip.clone()); } }
        }
        (vec![RequestData { method: "POST".to_string(), uri: self.endpoint.clone(), body, headers, imp_ids: get_imp_ids(&request.imp) }], vec![])
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(5);
        if let Some(cur) = &bid_resp.cur { result.currency = cur.clone(); }
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                result.bids.push(TypedBid::new(bid, BidType::Banner));
            }
        }
        Ok(result)
    }
}
