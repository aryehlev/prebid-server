use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
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
        for imp in &request.imp {
            let bidder = imp.ext.as_ref().and_then(|e| e.get("bidder"));
            let placement_id = bidder.and_then(|b| b.get("placementId")).and_then(|v| v.as_str()).unwrap_or("").to_string();
            let endpoint_id = bidder.and_then(|b| b.get("endpointId")).and_then(|v| v.as_str()).unwrap_or("").to_string();

            // Build imp.ext as {"bidder": {"type": ..., "placementId"|"endpointId": ...}}
            // Matches Go: if placementId set → type=publisher, else if endpointId set → type=network
            let bidder_ext = if !placement_id.is_empty() {
                serde_json::json!({
                    "type": "publisher",
                    "placementId": placement_id
                })
            } else if !endpoint_id.is_empty() {
                serde_json::json!({
                    "type": "network",
                    "endpointId": endpoint_id
                })
            } else {
                serde_json::json!({ "type": "" })
            };
            let new_imp_ext = serde_json::json!({ "bidder": bidder_ext });

            let mut imp_copy = imp.clone();
            imp_copy.ext = Some(new_imp_ext);
            let mut req_copy = request.clone();
            req_copy.imp = vec![imp_copy];
            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
            };
            let mut headers = HashMap::new();
            headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
            headers.insert("Accept".to_string(), "application/json".to_string());
            requests.push(RequestData {
                method: "POST".to_string(),
                uri: self.endpoint.clone(),
                body,
                headers,
                imp_ids: vec![imp.id.clone()],
            });
        }
        (requests, vec![])
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info.", response.status_code
            ))]);
        }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(internal.imp.len());
        // Pass through currency from response
        if let Some(cur) = &bid_resp.cur {
            if !cur.is_empty() {
                result.currency = cur.clone();
            }
        }
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                match get_media_type_for_imp(&bid.impid, &internal.imp) {
                    Ok(t) => result.bids.push(TypedBid::new(bid, t)),
                    Err(e) => return Err(vec![e]),
                }
            }
        }
        Ok(result)
    }
}
