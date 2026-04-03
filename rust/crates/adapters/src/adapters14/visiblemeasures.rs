use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct VisiblemeasuresAdapter { pub endpoint: String }
impl VisiblemeasuresAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

fn get_media_type_for_imp(imp_id: &str, imps: &[openrtb::Imp]) -> Result<BidType, BidderError> {
    for imp in imps {
        if imp.id == imp_id {
            if imp.banner.is_some() { return Ok(BidType::Banner); }
            if imp.video.is_some() { return Ok(BidType::Video); }
            if imp.native.is_some() { return Ok(BidType::Native); }
        }
    }
    Err(BidderError::BadInput(format!("Failed to find impression \"{}\"", imp_id)))
}

impl Bidder for VisiblemeasuresAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();
        let mut errs = Vec::new();
        for imp in &request.imp {
            let bidder = imp.ext.as_ref().and_then(|e| e.get("bidder"));
            let placement_id = bidder.and_then(|b| b.get("placementId")).and_then(|v| v.as_str()).unwrap_or("").to_string();
            let endpoint_id = bidder.and_then(|b| b.get("endpointId")).and_then(|v| v.as_str()).unwrap_or("").to_string();
            // Build ext with type field
            let (imp_type, type_val) = if !placement_id.is_empty() {
                ("placementId", placement_id.as_str())
            } else {
                ("endpointId", endpoint_id.as_str())
            };
            let ext_val = serde_json::json!({
                imp_type: type_val,
                "type": if !placement_id.is_empty() { "publisher" } else { "network" }
            });
            let mut imp_copy = imp.clone();
            imp_copy.ext = Some(ext_val);
            let mut req_copy = request.clone();
            req_copy.imp = vec![imp_copy];
            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };
            let mut headers = HashMap::new();
            headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
            headers.insert("Accept".to_string(), "application/json".to_string());
            requests.push(RequestData { method: "POST".to_string(), uri: self.endpoint.clone(), body, headers, imp_ids: vec![imp.id.clone()] });
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
                match get_media_type_for_imp(&bid.impid, &internal.imp) {
                    Ok(t) => result.bids.push(TypedBid::new(bid, t)),
                    Err(e) => errs.push(e),
                }
            }
        }
        if !errs.is_empty() && result.bids.is_empty() { return Err(errs); }
        Ok(result)
    }
}
