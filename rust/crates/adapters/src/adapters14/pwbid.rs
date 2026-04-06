use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct PwbidAdapter { pub endpoint: String }
impl PwbidAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

fn get_media_type_for_bid(impressions: &[openrtb::Imp], bid: &openrtb::Bid) -> Result<BidType, BidderError> {
    for imp in impressions {
        if imp.id == bid.impid {
            if imp.banner.is_some() { return Ok(BidType::Banner); }
            if imp.native.is_some() { return Ok(BidType::Native); }
            if imp.video.is_some() { return Ok(BidType::Video); }
        }
    }
    Err(BidderError::BadServerResponse(format!(
        "The impression with ID {} is not present into the request", bid.impid
    )))
}

impl Bidder for PwbidAdapter {
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
        let mut errors = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                match get_media_type_for_bid(&internal.imp, &bid) {
                    Ok(t) => result.bids.push(TypedBid::new(bid, t)),
                    Err(e) => errors.push(e),
                }
            }
        }
        Ok(result)
    }
}
