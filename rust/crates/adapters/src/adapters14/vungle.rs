use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use openrtb_ext::BidType;

const SUPPORTED_CURRENCY: &str = "USD";

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

        // Get buyer_uid from user for bid_token
        let buyer_uid = request.user.as_ref()
            .and_then(|u| u.buyeruid.as_deref())
            .unwrap_or("")
            .to_string();

        let mut request_copy = request.clone();

        for imp in &request.imp {
            // Handle bid floor currency conversion: only USD is supported.
            // If floor is set in a foreign currency we cannot convert, so skip.
            let mut imp_copy = imp.clone();
            if imp_copy.bidfloor.unwrap_or(0.0) > 0.0 {
                let cur = imp_copy.bidfloorcur.clone().unwrap_or_default().to_uppercase();
                if !cur.is_empty() && cur != SUPPORTED_CURRENCY {
                    errs.push(BidderError::BadInput(format!(
                        "failed to convert currency (err)cannot convert {} to {}", cur, SUPPORTED_CURRENCY
                    )));
                    continue;
                }
                imp_copy.bidfloorcur = Some(SUPPORTED_CURRENCY.to_string());
            }

            let bidder = imp.ext.as_ref().and_then(|e| e.get("bidder"));
            // Go ext JSON fields: placement_reference_id, app_store_id
            let placement_ref_id = bidder
                .and_then(|b| b.get("placement_reference_id"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let pub_app_store_id = bidder
                .and_then(|b| b.get("app_store_id"))
                .and_then(|v| v.as_str())
                .unwrap_or("");

            imp_copy.tagid = Some(placement_ref_id.to_string());

            // Build new imp ext: preserve original bidder ext plus add vungle key with bid_token
            let vungle_ext = serde_json::json!({
                "bidder": bidder,
                "vungle": {
                    "placement_reference_id": placement_ref_id,
                    "app_store_id": pub_app_store_id,
                    "bid_token": buyer_uid,
                }
            });
            imp_copy.ext = Some(vungle_ext);

            request_copy.imp = vec![imp_copy];

            // Construct app from existing app or create from site
            let app_obj = if let Some(existing_app) = &request.app {
                let mut a = existing_app.clone();
                a.id = Some(pub_app_store_id.to_string());
                a
            } else if request.site.is_some() {
                request_copy.site = None;
                openrtb::App { id: Some(pub_app_store_id.to_string()), ..Default::default() }
            } else {
                errs.push(BidderError::BadInput("failed constructing app, must have app or site object in bid request".to_string()));
                continue;
            };
            request_copy.app = Some(app_obj);

            let body = match serde_json::to_vec(&request_copy) {
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
                // Vungle always returns video bids
                result.bids.push(TypedBid::new(bid, BidType::Video));
            }
        }
        Ok(result)
    }
}
