use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;
use serde_json::Value;

pub struct AdyoulikeAdapter { pub endpoint: String }
impl AdyoulikeAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

#[derive(Deserialize)]
struct ExtImpBidder { bidder: Value }

#[derive(Deserialize)]
struct ExtImpAdyoulike {
    #[serde(default)]
    placement: String,
}

/// Returns the bid type for a given imp ID by inspecting the impression fields.
/// Matches Go logic: defaults to Banner unless banner is nil and video/native present.
fn get_media_type_for_imp(imp_id: &str, imps: &[openrtb::Imp]) -> BidType {
    for imp in imps {
        if imp.id == imp_id {
            if imp.banner.is_none() && imp.video.is_some() {
                return BidType::Video;
            }
            if imp.banner.is_none() && imp.native.is_some() {
                return BidType::Native;
            }
            return BidType::Banner;
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
            let placement = imp.ext.as_ref()
                .and_then(|e| serde_json::from_value::<ExtImpBidder>(e.clone()).ok())
                .and_then(|be| serde_json::from_value::<ExtImpAdyoulike>(be.bidder).ok())
                .map(|ext| ext.placement)
                .unwrap_or_default();
            if !placement.is_empty() {
                imp.tagid = Some(placement);
            } else {
                errs.push(BidderError::BadInput("placement not provided in imp ext".to_string()));
            }
        }

        // If any errors occurred, return them (matching Go behavior)
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
        match response.status_code {
            200 => {}
            204 => return Ok(BidderResponse::new()),
            400 => return Err(vec![BidderError::BadInput(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info",
                response.status_code
            ))]),
            _ => return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info",
                response.status_code
            ))]),
        }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let cap = request.imp.len();
        let mut result = BidderResponse::with_capacity(cap);
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
