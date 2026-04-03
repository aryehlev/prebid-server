use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct DecenteradsAdapter { pub endpoint: String }
impl DecenteradsAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

fn get_media_type_for_imp(imp_id: &str, imps: &[openrtb::Imp]) -> BidType {
    for imp in imps {
        if imp.id == imp_id {
            if imp.banner.is_some() { return BidType::Banner; }
            if imp.video.is_some() { return BidType::Video; }
            if imp.native.is_some() { return BidType::Native; }
            if imp.audio.is_some() { return BidType::Audio; }
        }
    }
    BidType::Banner
}

impl Bidder for DecenteradsAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        let impressions = request.imp.clone();
        let mut result = Vec::new();
        let mut errs = Vec::new();

        for imp in &impressions {
            // Extract bidder ext
            let imp_ext = match imp.ext.as_ref() {
                Some(e) => e.clone(),
                None => {
                    errs.push(BidderError::BadInput(format!("unable to parse bidder parameters: missing ext")));
                    continue;
                }
            };

            let bidder_ext = match imp_ext.get("bidder") {
                Some(v) if !v.is_null() => v.clone(),
                _ => {
                    errs.push(BidderError::BadInput("bidder parameters required".to_string()));
                    continue;
                }
            };

            // Build per-imp request with imp.Ext = bidderExt
            let mut req_copy = request.clone();
            let mut imp_copy = imp.clone();
            imp_copy.ext = Some(bidder_ext);
            req_copy.imp = vec![imp_copy];

            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };

            result.push(RequestData {
                method: "POST".to_string(),
                uri: self.endpoint.clone(),
                body,
                headers: headers.clone(),
                imp_ids: get_imp_ids(&req_copy.imp),
            });
        }

        (result, errs)
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(internal.imp.len());
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = get_media_type_for_imp(&bid.impid, &internal.imp);
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        Ok(result)
    }
}
