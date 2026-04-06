use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_bid_type_from_imp, get_imp_ids};
use openrtb::BidResponse;
use openrtb_ext::BidType;

pub struct BraveAdapter { pub endpoint: String }
impl BraveAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

impl Bidder for BraveAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::BadInput("no impressions".to_string())]);
        }

        // Extract placementId from first imp.ext.bidder
        let placement_id = request.imp[0].ext.as_ref()
            .and_then(|e| e.get("bidder"))
            .and_then(|b| b.get("placementId"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if placement_id.is_empty() {
            return (vec![], vec![BidderError::BadInput("ext.bidder not provided".to_string())]);
        }

        // Build URL: replace {{.PublisherID}} with placementId
        let url = self.endpoint.replace("{{.PublisherID}}", placement_id);

        // Clear first imp ext
        let mut req = request.clone();
        req.imp[0].ext = None;

        let body = match serde_json::to_vec(&req) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        (vec![RequestData {
            method: "POST".to_string(),
            uri: url,
            body,
            headers,
            imp_ids: get_imp_ids(&request.imp),
        }], vec![])
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 {
            return Err(vec![BidderError::BadInput("No bid".to_string())]);
        }
        if response.status_code == 400 {
            return Err(vec![BidderError::BadInput(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info", response.status_code
            ))]);
        }
        if response.status_code == 503 {
            return Err(vec![BidderError::BadInput(format!(
                "Service Unavailable. Status Code: [ {} ] ", response.status_code
            ))]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Something went wrong, please contact your Account Manager. Status Code: [ {} ] ", response.status_code
            ))]);
        }

        let bid_resp: BidResponse = serde_json::from_slice(&response.body)
            .map_err(|_| vec![BidderError::BadServerResponse("Bad Server Response".to_string())])?;

        if bid_resp.seatbid.is_empty() {
            return Err(vec![BidderError::BadServerResponse("Empty SeatBid array".to_string())]);
        }

        let mut result = BidderResponse::with_capacity(bid_resp.seatbid[0].bid.len());
        for bid in &bid_resp.seatbid[0].bid {
            let bid_type = internal.imp.iter()
                .find(|i| i.id == bid.impid)
                .map(get_bid_type_from_imp)
                .unwrap_or(BidType::Banner);
            result.bids.push(TypedBid::new(bid.clone(), bid_type));
        }
        Ok(result)
    }
}
