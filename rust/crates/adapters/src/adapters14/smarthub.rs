use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct SmarthubAdapter { pub endpoint: String }
impl SmarthubAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

fn parse_bid_type(s: &str) -> Result<BidType, BidderError> {
    match s {
        "banner" => Ok(BidType::Banner),
        "video" => Ok(BidType::Video),
        "native" => Ok(BidType::Native),
        "audio" => Ok(BidType::Audio),
        other => Err(BidderError::BadServerResponse(format!("unknown bid type: {}", other))),
    }
}

impl Bidder for SmarthubAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::BadInput("no impressions".to_string())]);
        }
        let bidder = request.imp[0].ext.as_ref().and_then(|e| e.get("bidder"));
        let seat = bidder.and_then(|b| b.get("seat")).and_then(|v| v.as_str()).unwrap_or("");
        let token = bidder.and_then(|b| b.get("token")).and_then(|v| v.as_str()).unwrap_or("");
        let partner_name = bidder.and_then(|b| b.get("partnerName")).and_then(|v| v.as_str()).unwrap_or("");
        let url = self.endpoint
            .replace("{{.AccountID}}", seat)
            .replace("{{.SourceId}}", token)
            .replace("{{.Host}}", partner_name);
        let body = match serde_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
        (vec![RequestData { method: "POST".to_string(), uri: url, body, headers, imp_ids: get_imp_ids(&request.imp) }], vec![])
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        if bid_resp.seatbid.is_empty() {
            return Err(vec![BidderError::BadServerResponse("Array SeatBid cannot be empty".to_string())]);
        }
        let mut result = BidderResponse::with_capacity(5);
        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let media_type = bid.ext.as_ref()
                    .and_then(|e| e.get("mediaType"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                match parse_bid_type(media_type) {
                    Ok(t) => result.bids.push(TypedBid::new(bid, t)),
                    Err(e) => errs.push(e),
                }
            }
        }
        if !errs.is_empty() && result.bids.is_empty() { return Err(errs); }
        Ok(result)
    }
}
