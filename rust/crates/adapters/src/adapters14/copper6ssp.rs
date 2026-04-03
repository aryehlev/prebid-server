use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct Copper6sspAdapter { pub endpoint: String }
impl Copper6sspAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

fn get_bid_type_from_mtype(mtype: i32) -> Result<BidType, BidderError> {
    match mtype {
        1 => Ok(BidType::Banner),
        2 => Ok(BidType::Video),
        4 => Ok(BidType::Native),
        _ => Err(BidderError::BadServerResponse(
            format!("could not define media type for impression with mtype {}", mtype)
        )),
    }
}

impl Bidder for Copper6sspAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();
        let mut errs = Vec::new();

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        for imp in &request.imp {
            let bidder = imp.ext.as_ref()
                .and_then(|e| e.get("bidder"))
                .cloned()
                .unwrap_or(serde_json::Value::Null);

            let placement_id = bidder.get("placementId").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let endpoint_id = bidder.get("endpointId").and_then(|v| v.as_str()).unwrap_or("").to_string();

            // Build bidder ext with type
            let bidder_ext = if !placement_id.is_empty() {
                serde_json::json!({
                    "bidder": {
                        "type": "publisher",
                        "placementId": placement_id
                    }
                })
            } else if !endpoint_id.is_empty() {
                serde_json::json!({
                    "bidder": {
                        "type": "network",
                        "endpointId": endpoint_id
                    }
                })
            } else {
                serde_json::json!({ "bidder": { "type": "" } })
            };

            let mut imp_copy = imp.clone();
            imp_copy.ext = Some(bidder_ext);

            let mut req_copy = request.clone();
            req_copy.imp = vec![imp_copy];

            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };

            requests.push(RequestData {
                method: "POST".to_string(),
                uri: self.endpoint.clone(),
                body,
                headers: headers.clone(),
                imp_ids: vec![imp.id.clone()],
            });
        }

        if requests.is_empty() {
            (vec![], errs)
        } else {
            (requests, errs)
        }
    }

    fn make_bids(&self, request: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(request.imp.len());
        if let Some(cur) = &bid_resp.cur {
            if !cur.is_empty() {
                result.currency = cur.clone();
            }
        }

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let mtype = bid.mtype.unwrap_or(0);
                let bid_type = get_bid_type_from_mtype(mtype)
                    .map_err(|e| vec![e])?;
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        Ok(result)
    }
}
