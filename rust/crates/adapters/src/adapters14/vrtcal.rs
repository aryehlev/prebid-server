use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_bid_type_from_mtype, get_imp_ids};

pub struct VrtcalAdapter { pub endpoint: String }
impl VrtcalAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

impl Bidder for VrtcalAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let body = match serde_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        (vec![RequestData { method: "POST".to_string(), uri: self.endpoint.clone(), body, headers, imp_ids: get_imp_ids(&request.imp) }], vec![])
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code == 400 {
            return Err(vec![BidderError::BadInput(
                format!("Unexpected status code: {}. Run with request.debug = 1 for more info", response.status_code)
            )]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(
                format!("Unexpected status code: {}. Run with request.debug = 1 for more info", response.status_code)
            )]);
        }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(1);
        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let mtype = bid.mtype.unwrap_or(0);
                // Supported: banner(1), video(2), native(4)
                if mtype == 1 || mtype == 2 || mtype == 4 {
                    result.bids.push(TypedBid::new(bid, get_bid_type_from_mtype(mtype)));
                } else {
                    errs.push(BidderError::BadServerResponse("Unsupported return type".to_string()));
                }
            }
        }
        // Go returns both response and errs - we map to Ok if there are bids
        if !result.bids.is_empty() || errs.is_empty() {
            Ok(result)
        } else {
            Err(errs)
        }
    }
}
