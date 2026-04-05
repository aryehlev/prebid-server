use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct SmartrtbAdapter { pub endpoint: String }
impl SmartrtbAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

impl Bidder for SmartrtbAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        // Collect valid imps (banner or video only) and extract pub_id/zone_id
        let mut valid_imps = Vec::new();
        let mut pub_id = String::new();
        let mut errs = Vec::new();
        for imp in &request.imp {
            if imp.banner.is_none() && imp.video.is_none() { continue; }
            let bidder = imp.ext.as_ref().and_then(|e| e.get("bidder"));
            let imp_pub_id = bidder.and_then(|b| b.get("pub_id")).and_then(|v| v.as_str()).unwrap_or("").to_string();
            let zone_id = bidder.and_then(|b| b.get("zone_id")).and_then(|v| v.as_str()).unwrap_or("").to_string();
            if pub_id.is_empty() { pub_id = imp_pub_id; }
            let mut imp_copy = imp.clone();
            if !zone_id.is_empty() { imp_copy.tagid = Some(zone_id); }
            valid_imps.push(imp_copy);
        }
        if valid_imps.is_empty() {
            return (vec![], errs);
        }
        if pub_id.is_empty() {
            errs.push(BidderError::BadInput("Cannot infer publisher ID from bid ext".to_string()));
            return (vec![], errs);
        }
        let uri = self.endpoint.replace("{{.PublisherID}}", &pub_id);
        // Build request ext with pub_id
        let req_ext = serde_json::json!({ "pub_id": pub_id });
        let mut req_copy = request.clone();
        req_copy.imp = valid_imps;
        req_copy.ext = Some(req_ext);
        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
        headers.insert("x-openrtb-version".to_string(), "2.5".to_string());
        (vec![RequestData { method: "POST".to_string(), uri, body, headers, imp_ids: get_imp_ids(&req_copy.imp) }], errs)
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code == 400 {
            return Err(vec![BidderError::BadInput("Invalid request.".to_string())]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!("Unexpected HTTP status {}.", response.status_code))]);
        }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(5);
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                // bid.ext.format contains creative type: BANNER, VIDEO
                let creative_type = bid.ext.as_ref()
                    .and_then(|e| e.get("format"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let bid_type = match creative_type {
                    "BANNER" => BidType::Banner,
                    "VIDEO" => BidType::Video,
                    other => {
                        return Err(vec![BidderError::BadServerResponse(format!("Unsupported creative type {}.", other))]);
                    }
                };
                let mut bid = bid;
                bid.ext = None;
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        Ok(result)
    }
}
