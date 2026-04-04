use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct VideoHeroesAdapter { pub endpoint: String }
impl VideoHeroesAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

fn get_media_type_for_imp(imp_id: &str, imps: &[openrtb::Imp]) -> BidType {
    for imp in imps {
        if imp.id == imp_id {
            if imp.video.is_some() { return BidType::Video; }
            if imp.native.is_some() { return BidType::Native; }
            return BidType::Banner;
        }
    }
    BidType::Banner
}

impl Bidder for VideoHeroesAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::BadInput("no impressions".to_string())]);
        }
        let bidder = request.imp[0].ext.as_ref().and_then(|e| e.get("bidder"));
        let placement_id = bidder.and_then(|b| b.get("placementId")).and_then(|v| v.as_str()).unwrap_or("");
        if placement_id.is_empty() {
            return (vec![], vec![BidderError::BadInput("ext.bidder not provided".to_string())]);
        }
        let url = self.endpoint.replace("{{.PublisherID}}", placement_id);
        let mut req_copy = request.clone();
        req_copy.imp[0].ext = None;
        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
        (vec![RequestData { method: "POST".to_string(), uri: url, body, headers, imp_ids: get_imp_ids(&request.imp) }], vec![])
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        match response.status_code {
            204 => return Err(vec![BidderError::BadInput("No bid".to_string())]),
            400 => return Err(vec![BidderError::BadInput(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info", response.status_code
            ))]),
            503 => return Err(vec![BidderError::BadInput(format!(
                "Service Unavailable. Status Code: [ {} ] ", response.status_code
            ))]),
            200 => {}
            _ => return Err(vec![BidderError::BadServerResponse(format!(
                "Something went wrong, please contact your Account Manager. Status Code: [ {} ] ", response.status_code
            ))]),
        }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|_| vec![BidderError::BadServerResponse("Bad Server Response".to_string())])?;
        if bid_resp.seatbid.is_empty() {
            return Err(vec![BidderError::BadServerResponse("Empty SeatBid array".to_string())]);
        }
        let mut result = BidderResponse::with_capacity(bid_resp.seatbid[0].bid.len());
        let sb = &bid_resp.seatbid[0];
        for bid in &sb.bid {
            let bid_type = get_media_type_for_imp(&bid.impid, &internal.imp);
            result.bids.push(TypedBid::new(bid.clone(), bid_type));
        }
        Ok(result)
    }
}
