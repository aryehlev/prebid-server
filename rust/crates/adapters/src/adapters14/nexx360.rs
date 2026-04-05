use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct Nexx360Adapter { pub endpoint: String }
impl Nexx360Adapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

fn get_bid_type(bid: &openrtb::Bid) -> Result<BidType, BidderError> {
    let t = bid.ext.as_ref().and_then(|e| e.get("bidType")).and_then(|v| v.as_str()).unwrap_or("");
    match t {
        "video" => Ok(BidType::Video),
        "audio" => Ok(BidType::Audio),
        "native" => Ok(BidType::Native),
        "banner" => Ok(BidType::Banner),
        _ => Err(BidderError::BadServerResponse(format!("unable to fetch mediaType in multi-format: {}", bid.impid))),
    }
}

/// Build the request ext with nexx360 caller info.
fn make_req_ext() -> serde_json::Value {
    serde_json::json!({
        "nexx360": {
            "caller": [
                { "name": "Prebid-Server", "version": "n/a" }
            ]
        }
    })
}

impl Bidder for Nexx360Adapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        // Process imps: replace bidder ext with nexx360 ext
        let mut imps = Vec::new();
        let mut tag_id = String::new();
        let mut placement = String::new();
        let mut errs = Vec::new();

        for (idx, imp) in request.imp.iter().enumerate() {
            let bidder_val = match imp.ext.as_ref().and_then(|e| e.get("bidder")).cloned() {
                Some(v) => v,
                None => { errs.push(BidderError::BadInput("missing bidder ext".to_string())); return (vec![], errs); }
            };
            if idx == 0 {
                tag_id = bidder_val.get("tagId").and_then(|v| v.as_str()).unwrap_or("").to_string();
                placement = bidder_val.get("placement").and_then(|v| v.as_str()).unwrap_or("").to_string();
            }
            // Build imp.ext with nexx360 key instead of bidder
            let mut ext_map = imp.ext.as_ref().and_then(|e| e.as_object()).cloned().unwrap_or_default();
            ext_map.remove("bidder");
            ext_map.insert("nexx360".to_string(), bidder_val);
            let mut imp_copy = imp.clone();
            imp_copy.ext = Some(serde_json::Value::Object(ext_map));
            imps.push(imp_copy);
        }

        // Build URL with query params
        let mut uri = self.endpoint.clone();
        let mut params = Vec::new();
        if !placement.is_empty() { params.push(format!("placement={}", placement)); }
        if !tag_id.is_empty() { params.push(format!("tag_id={}", tag_id)); }
        if !params.is_empty() { uri = format!("{}?{}", uri, params.join("&")); }

        let mut req_copy = request.clone();
        req_copy.imp = imps;

        // Set request ext with nexx360 caller info
        req_copy.ext = Some(make_req_ext());

        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json".to_string());
        (vec![RequestData { method: "POST".to_string(), uri, body, headers, imp_ids: get_imp_ids(&request.imp) }], errs)
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadInput(format!("Unexpected http status code: {}", response.status_code))]);
        }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        if bid_resp.seatbid.is_empty() { return Ok(BidderResponse::new()); }
        let mut bids = Vec::new();
        let mut errors = Vec::new();
        for sb in &bid_resp.seatbid {
            for bid in &sb.bid {
                match get_bid_type(bid) {
                    Ok(t) => bids.push(TypedBid::new(bid.clone(), t)),
                    Err(e) => errors.push(e),
                }
            }
        }
        if bids.is_empty() { return Ok(BidderResponse::new()); }
        let mut result = BidderResponse::with_capacity(bids.len());
        if let Some(cur) = &bid_resp.cur { if !cur.is_empty() { result.currency = cur.clone(); } }
        result.bids = bids;
        Ok(result)
    }
}
