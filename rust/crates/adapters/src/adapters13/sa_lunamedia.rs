use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb::BidResponse;
use openrtb_ext::BidType;

pub struct SaLunamediaAdapter { pub endpoint: String }
impl SaLunamediaAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

fn parse_bid_type(media_type: &str) -> Result<BidType, BidderError> {
    match media_type {
        "banner" => Ok(BidType::Banner),
        "video" => Ok(BidType::Video),
        "native" => Ok(BidType::Native),
        _ => Err(BidderError::BadServerResponse(format!("invalid bid type: {}", media_type))),
    }
}

impl Bidder for SaLunamediaAdapter {
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
            return Err(vec![BidderError::BadInput(
                format!("Bad Request. {}", String::from_utf8_lossy(&response.body))
            )]);
        }
        if response.status_code == 503 {
            return Err(vec![BidderError::BadInput(
                "Bidder unavailable. Please contact the bidder support.".to_string()
            )]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(
                format!("Status Code: [ {} ] {}", response.status_code, String::from_utf8_lossy(&response.body))
            )]);
        }
        let bid_resp: BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        if bid_resp.seatbid.is_empty() {
            return Err(vec![BidderError::BadServerResponse("Empty SeatBid".to_string())]);
        }
        let bids = &bid_resp.seatbid[0].bid;
        if bids.is_empty() {
            return Err(vec![BidderError::BadServerResponse("Empty SeatBid.Bids".to_string())]);
        }
        let bid = bids[0].clone();
        // Get media_type from bid.ext.mediaType
        let media_type = bid.ext.as_ref()
            .and_then(|e| e.get("mediaType"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if media_type.is_empty() {
            return Err(vec![BidderError::BadServerResponse("Missing BidExt".to_string())]);
        }
        let bid_type = parse_bid_type(media_type)
            .map_err(|e| vec![e])?;
        let mut result = BidderResponse::with_capacity(1);
        result.bids.push(TypedBid::new(bid, bid_type));
        Ok(result)
    }
}
