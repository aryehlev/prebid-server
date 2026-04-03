use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids, check_response_status};
use openrtb::BidResponse;
use openrtb_ext::BidType;

pub struct EvolutionAdapter { pub endpoint: String }
impl EvolutionAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

/// Read bid type from bid.ext.mediaType field (e_volution custom).
fn get_bid_type_from_bid_ext(bid: &openrtb::Bid) -> BidType {
    let type_str = bid.ext.as_ref()
        .and_then(|e| e.get("mediaType"))
        .and_then(|v| v.as_str())
        .unwrap_or("banner");
    match type_str {
        "video" => BidType::Video,
        "native" => BidType::Native,
        _ => BidType::Banner,
    }
}

impl Bidder for EvolutionAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let body = match serde_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
        (vec![RequestData {
            method: "POST".to_string(),
            uri: self.endpoint.clone(),
            body,
            headers,
            imp_ids: get_imp_ids(&request.imp),
        }], vec![])
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code == 400 {
            return Err(vec![BidderError::BadInput(format!(
                "Bad Request. {}", String::from_utf8_lossy(&response.body)
            ))]);
        }
        if response.status_code == 503 { return Ok(BidderResponse::new()); }
        if let Err(e) = check_response_status(response.status_code) { return Err(vec![e]); }

        let bid_resp: BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(format!("Bad response, {}", e))])?;

        if bid_resp.seatbid.is_empty() {
            return Err(vec![BidderError::BadServerResponse("Empty seatbid".to_string())]);
        }

        let mut result = BidderResponse::with_capacity(bid_resp.seatbid[0].bid.len());
        for bid in &bid_resp.seatbid[0].bid {
            let bid_type = get_bid_type_from_bid_ext(bid);
            result.bids.push(TypedBid::new(bid.clone(), bid_type));
        }
        Ok(result)
    }
}
