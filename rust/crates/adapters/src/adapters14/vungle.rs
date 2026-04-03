use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct VungleAdapter { pub endpoint: String }
impl VungleAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

impl Bidder for VungleAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();
        let mut errs = Vec::new();
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
        headers.insert("X-OpenRTB-Version".to_string(), "2.5".to_string());

        for imp in &request.imp {
            let bidder = imp.ext.as_ref().and_then(|e| e.get("bidder"));
            let placement_ref_id = bidder
                .and_then(|b| b.get("placementRefId"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let pub_app_store_id = bidder
                .and_then(|b| b.get("pubAppStoreID"))
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let mut imp_copy = imp.clone();
            imp_copy.tagid = Some(placement_ref_id.to_string());
            imp_copy.ext = None;

            let mut req_copy = request.clone();
            req_copy.imp = vec![imp_copy];

            // Construct app from existing app or create from site
            let mut app = if let Some(existing_app) = &request.app {
                let mut a = existing_app.clone();
                a.id = Some(pub_app_store_id.to_string());
                a
            } else if request.site.is_some() {
                req_copy.site = None;
                openrtb::App { id: Some(pub_app_store_id.to_string()), ..Default::default() }
            } else {
                errs.push(BidderError::BadInput("failed constructing app, must have app or site object in bid request".to_string()));
                continue;
            };
            req_copy.app = Some(app);

            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };
            requests.push(RequestData { method: "POST".to_string(), uri: self.endpoint.clone(), body, headers: headers.clone(), imp_ids: vec![imp.id.clone()] });
        }
        (requests, errs)
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(5);
        if let Some(cur) = &bid_resp.cur { result.currency = cur.clone(); }
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                result.bids.push(TypedBid::new(bid, BidType::Video));
            }
        }
        Ok(result)
    }
}
