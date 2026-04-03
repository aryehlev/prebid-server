use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids, check_response_status};
use openrtb::BidResponse;
use openrtb_ext::BidType;

pub struct GlobalsunAdapter { pub endpoint: String }
impl GlobalsunAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

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

impl Bidder for GlobalsunAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();

        for imp in &request.imp {
            let placement_id = imp.ext.as_ref()
                .and_then(|e| e.get("bidder"))
                .and_then(|b| b.get("placementId"))
                .and_then(|v| v.as_str())
                .unwrap_or("");

            if placement_id.is_empty() {
                return (vec![], vec![BidderError::BadInput(
                    "missing placementId in imp.ext.bidder".to_string()
                )]);
            }

            let new_ext = serde_json::json!({
                "bidder": {
                    "placementId": placement_id,
                    "type": "publisher"
                }
            });

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

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
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
                let bid_type = get_media_type_for_imp(&bid.impid, &internal.imp)
                    .map_err(|e| vec![e])?;
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        Ok(result)
    }
}
