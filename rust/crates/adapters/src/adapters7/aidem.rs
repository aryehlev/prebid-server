use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use openrtb_ext::BidType;
use serde::Deserialize;
use serde_json::Value;

pub struct AidemAdapter { pub endpoint: String }
impl AidemAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

#[derive(Deserialize)]
struct ExtImpBidder { bidder: Value }

#[derive(Deserialize)]
struct ExtImpAidem {
    #[serde(rename = "publisherId", default)]
    publisher_id: String,
}

fn mtype_to_bid_type(mtype: Option<i32>, imp_id: &str) -> Result<BidType, BidderError> {
    match mtype {
        Some(1) => Ok(BidType::Banner),
        Some(2) => Ok(BidType::Video),
        _ => Err(BidderError::BadInput(format!("Unable to fetch mediaType in multi-format: {}", imp_id))),
    }
}

impl Bidder for AidemAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::BadInput("no impressions".to_string())]);
        }
        let imp = &request.imp[0];
        let bidder_ext: ExtImpBidder = match imp.ext.as_ref()
            .and_then(|e| serde_json::from_value(e.clone()).ok()) {
            Some(v) => v,
            None => return (vec![], vec![BidderError::BadInput("ext.bidder not provided".to_string())]),
        };
        let ext: ExtImpAidem = match serde_json::from_value(bidder_ext.bidder) {
            Ok(v) => v,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };
        let uri = self.endpoint.replace("{{.PublisherID}}", &ext.publisher_id);
        let body = match serde_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        let imp_ids: Vec<String> = request.imp.iter().map(|i| i.id.clone()).collect();
        (vec![RequestData { method: "POST".to_string(), uri, body, headers, imp_ids }], vec![])
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(_) = crate::check_response_status(response.status_code) {
            return Err(vec![BidderError::BadInput(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info", response.status_code
            ))]);
        }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(format!("JSON parsing error: {}", e))])?;
        if bid_resp.seatbid.is_empty() {
            return Err(vec![BidderError::BadServerResponse("Empty SeatBid array".to_string())]);
        }
        let mut result = BidderResponse::with_capacity(bid_resp.seatbid[0].bid.len());
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                match mtype_to_bid_type(bid.mtype, &bid.impid) {
                    Ok(bt) => result.bids.push(TypedBid::new(bid, bt)),
                    Err(_) => {} // non-fatal, skip bid with unknown mtype
                }
            }
        }
        Ok(result)
    }
}
