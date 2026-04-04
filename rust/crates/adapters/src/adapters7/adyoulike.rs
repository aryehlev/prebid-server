use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_bid_type_from_imp, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;
use serde_json::Value;

pub struct AdyoulikeAdapter { pub endpoint: String }
impl AdyoulikeAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

#[derive(Deserialize)]
struct ExtImpBidder { bidder: Value }

#[derive(Deserialize)]
struct ExtImpAdyoulike {
    #[serde(rename = "placement", default)]
    placement_id: String,
}

/// Returns the bid type for a given imp ID by inspecting the impression fields.
/// Defaults to Banner if no video or native object is present.
fn get_media_type_for_imp(imp_id: &str, imps: &[openrtb::Imp]) -> BidType {
    for imp in imps {
        if imp.id == imp_id {
            return get_bid_type_from_imp(imp);
        }
    }
    BidType::Banner
}

impl Bidder for AdyoulikeAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut req = request.clone();
        // Force USD as the request currency
        req.cur = Some(vec!["USD".to_string()]);

        let mut errs: Vec<BidderError> = Vec::new();
        for imp in &mut req.imp {
            // Extract placement from imp.ext.bidder.placement and set as tagid
            let tag_id = imp.ext.as_ref()
                .and_then(|e| serde_json::from_value::<ExtImpBidder>(e.clone()).ok())
                .and_then(|be| serde_json::from_value::<ExtImpAdyoulike>(be.bidder).ok())
                .map(|ext| ext.placement_id)
                .unwrap_or_default();
            if !tag_id.is_empty() {
                imp.tagid = Some(tag_id);
            } else {
                errs.push(BidderError::BadInput("placement not provided in imp ext".to_string()));
            }
        }

        if !errs.is_empty() {
            return (vec![], errs);
        }

        let body = match serde_json::to_vec(&req) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
        headers.insert("x-openrtb-version".to_string(), "2.5".to_string());

        (vec![RequestData { method: "POST".to_string(), uri: self.endpoint.clone(), body, headers, imp_ids: get_imp_ids(&request.imp) }], vec![])
    }

    fn make_bids(&self, request: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(5);
        result.currency = "USD".to_string();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = get_media_type_for_imp(&bid.impid, &request.imp);
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        Ok(result)
    }
}
