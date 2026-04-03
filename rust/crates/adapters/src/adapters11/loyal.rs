use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids, check_response_status};
use openrtb::BidResponse;
use openrtb_ext::BidType;

pub struct LoyalAdapter { pub endpoint: String }
impl LoyalAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

/// Rewrite imp.ext.bidder to {"bidder": {"type": ..., "placementId": ...}} or endpointId form.
fn build_imp_ext(imp: &openrtb::Imp) -> Result<serde_json::Value, BidderError> {
    let bidder_ext = imp.ext.as_ref()
        .and_then(|e| e.get("bidder"))
        .cloned()
        .unwrap_or(serde_json::Value::Null);

    let placement_id = bidder_ext.get("placementId").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let endpoint_id = bidder_ext.get("endpointId").and_then(|v| v.as_str()).unwrap_or("").to_string();

    let mut inner = serde_json::json!({});
    if !placement_id.is_empty() {
        inner["placementId"] = serde_json::Value::String(placement_id);
        inner["type"] = serde_json::Value::String("publisher".to_string());
    } else if !endpoint_id.is_empty() {
        inner["endpointId"] = serde_json::Value::String(endpoint_id);
        inner["type"] = serde_json::Value::String("network".to_string());
    }

    Ok(serde_json::json!({ "bidder": inner }))
}

fn get_bid_type_from_bid_ext(bid: &openrtb::Bid) -> Result<BidType, BidderError> {
    let type_str = bid.ext.as_ref()
        .and_then(|e| e.get("prebid"))
        .and_then(|p| p.get("type"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    match type_str {
        "banner" => Ok(BidType::Banner),
        "video" => Ok(BidType::Video),
        "native" => Ok(BidType::Native),
        _ => Err(BidderError::BadServerResponse(format!("invalid BidType: {}", type_str))),
    }
}

impl Bidder for LoyalAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut requests = Vec::new();

        for imp in &request.imp {
            let new_ext = match build_imp_ext(imp) {
                Ok(e) => e,
                Err(e) => { errs.push(e); continue; }
            };

            let mut imp_copy = imp.clone();
            imp_copy.ext = Some(new_ext);

            let mut req_copy = request.clone();
            req_copy.imp = vec![imp_copy];

            let imp_ids = get_imp_ids(&req_copy.imp);
            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };

            let mut headers = HashMap::new();
            headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
            headers.insert("Accept".to_string(), "application/json".to_string());

            requests.push(RequestData {
                method: "POST".to_string(),
                uri: self.endpoint.clone(),
                body,
                headers,
                imp_ids,
            });
        }

        if requests.is_empty() {
            errs.push(BidderError::BadInput("found no valid impressions".to_string()));
            return (vec![], errs);
        }

        (requests, errs)
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(5);
        if let Some(cur) = bid_resp.cur.filter(|c| !c.is_empty()) {
            result.currency = cur;
        }
        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                match get_bid_type_from_bid_ext(&bid) {
                    Ok(t) => result.bids.push(TypedBid::new(bid, t)),
                    Err(e) => errs.push(e),
                }
            }
        }
        if !errs.is_empty() {
            return Err(errs);
        }
        Ok(result)
    }
}
