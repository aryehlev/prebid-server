use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids, check_response_status};
use openrtb::BidResponse;
use openrtb_ext::BidType;

pub struct PgamsspAdapter { pub endpoint: String }
impl PgamsspAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

fn get_bid_type_from_mtype(mtype: u64, imp_id: &str) -> Result<BidType, BidderError> {
    match mtype {
        1 => Ok(BidType::Banner),
        2 => Ok(BidType::Video),
        3 => Ok(BidType::Audio),
        4 => Ok(BidType::Native),
        _ => Err(BidderError::BadServerResponse(
            format!("Unable to fetch mediaType in multi-format: {}", imp_id)
        )),
    }
}

fn build_imp_ext(imp: &openrtb::Imp) -> Result<serde_json::Value, BidderError> {
    let bidder = imp.ext.as_ref()
        .and_then(|e| e.get("bidder"))
        .cloned()
        .unwrap_or(serde_json::Value::Null);

    let placement_id = bidder.get("placementId").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let endpoint_id = bidder.get("endpointId").and_then(|v| v.as_str()).unwrap_or("").to_string();

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

impl Bidder for PgamsspAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();

        for imp in &request.imp {
            let new_ext = match build_imp_ext(imp) {
                Ok(e) => e,
                Err(e) => return (vec![], vec![e]),
            };

            let mut imp_copy = imp.clone();
            imp_copy.ext = Some(new_ext);

            let mut req_copy = request.clone();
            req_copy.imp = vec![imp_copy];

            let imp_ids = get_imp_ids(&req_copy.imp);
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
                imp_ids,
            });
        }

        (requests, vec![])
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(5);
        if let Some(cur) = &bid_resp.cur {
            if !cur.is_empty() { result.currency = cur.clone(); }
        }
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let mtype = bid.ext.as_ref()
                    .and_then(|e| e.get("mtype"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let bid_type = get_bid_type_from_mtype(mtype, &bid.impid)
                    .map_err(|e| vec![e])?;
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        Ok(result)
    }
}
