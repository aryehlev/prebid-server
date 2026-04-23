use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct MobkoiAdapter { pub endpoint: String }
impl MobkoiAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

impl Bidder for MobkoiAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::BadInput("no impressions".to_string())]);
        }

        // Extract placementId from imp[0].ext.bidder
        let bidder = request.imp[0].ext.as_ref().and_then(|e| e.get("bidder"));
        let placement_id = bidder
            .and_then(|b| b.get("placementId"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // If placementId present, use it as tagID; otherwise use existing tagId.
        // If neither exists, fail with BadInput.
        let tag_id = if !placement_id.is_empty() {
            placement_id
        } else {
            request.imp[0].tagid.clone().unwrap_or_default()
        };

        if tag_id.is_empty() {
            return (vec![], vec![BidderError::BadInput(
                "invalid because it comes with neither request.imp[0].tagId nor req.imp[0].ext.Bidder.placementId".to_string()
            )]);
        }

        let mut req_copy = request.clone();
        req_copy.imp[0].tagid = Some(tag_id);

        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        (vec![RequestData {
            method: "POST".to_string(),
            uri: self.endpoint.clone(),
            body,
            headers,
            imp_ids: get_imp_ids(&req_copy.imp),
        }], vec![])
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(1);
        if let Some(cur) = &bid_resp.cur { result.currency = cur.clone(); }

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                // mobkoi only serves banner ads
                result.bids.push(TypedBid::new(bid, BidType::Banner));
            }
        }
        Ok(result)
    }
}
