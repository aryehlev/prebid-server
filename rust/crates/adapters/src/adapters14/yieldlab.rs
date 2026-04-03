use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct YieldlabAdapter { pub endpoint: String }
impl YieldlabAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

impl Bidder for YieldlabAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::BadInput("no impressions given".to_string())]);
        }
        let mut headers = HashMap::new();
        headers.insert("Accept".to_string(), "application/json".to_string());
        if let Some(site) = &request.site {
            if let Some(page) = &site.page { headers.insert("Referer".to_string(), page.clone()); }
        }
        if let Some(device) = &request.device {
            if let Some(ua) = &device.ua { headers.insert("User-Agent".to_string(), ua.clone()); }
            if let Some(ip) = &device.ip { headers.insert("X-Forwarded-For".to_string(), ip.clone()); }
        }
        // Build GET request - use endpoint as-is
        (vec![RequestData { method: "GET".to_string(), uri: self.endpoint.clone(), body: vec![], headers, imp_ids: get_imp_ids(&request.imp) }], vec![])
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(5);
        result.currency = "EUR".to_string();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = internal.imp.iter().find(|i| i.id == bid.impid).map(|imp| {
                    if imp.video.is_some() { BidType::Video } else { BidType::Banner }
                }).unwrap_or(BidType::Banner);
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        Ok(result)
    }
}
