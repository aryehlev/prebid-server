use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct NextmillenniumAdapter { pub endpoint: String }
impl NextmillenniumAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

impl Bidder for NextmillenniumAdapter {
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

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        if bid_resp.seatbid.is_empty() { return Ok(BidderResponse::new()); }
        let mut result = BidderResponse::with_capacity(1);
        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = match bid.mtype.unwrap_or(0) {
                    1 => BidType::Banner,
                    2 => BidType::Video,
                    m => {
                        errs.push(BidderError::BadServerResponse(format!("Unsupported mType: {}", m)));
                        continue;
                    }
                };
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        if !errs.is_empty() && result.bids.is_empty() { return Err(errs); }
        Ok(result)
    }
}
