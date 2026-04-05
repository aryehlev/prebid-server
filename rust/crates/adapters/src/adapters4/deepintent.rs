use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;

const DISPLAY_MANAGER: &str = "di_prebid";
const DISPLAY_MANAGER_VER: &str = "2.0.0";

pub struct DeepintentAdapter { pub endpoint: String }
impl DeepintentAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

#[derive(Debug, Default, Deserialize)]
struct ExtImpDeepintent {
    #[serde(rename = "tagId", default)]
    tag_id: String,
}

fn build_imp_banner(imp: &mut openrtb::Imp) -> Result<(), BidderError> {
    let banner = imp.banner.as_mut().ok_or_else(|| {
        BidderError::BadInput("We need a Banner Object in the request".to_string())
    })?;

    if banner.w.is_none() && banner.h.is_none() {
        let format = banner.format.as_ref()
            .and_then(|f| f.first())
            .ok_or_else(|| BidderError::BadInput("At least one size is required".to_string()))?;
        banner.w = format.w;
        banner.h = format.h;
    }
    Ok(())
}

fn get_media_type_for_imp(imp_id: &str, imps: &[openrtb::Imp]) -> Result<BidType, BidderError> {
    for imp in imps {
        if imp.id == imp_id {
            return Ok(BidType::Banner);
        }
    }
    Err(BidderError::BadInput(format!("Failed to find impression {} ", imp_id)))
}

impl Bidder for DeepintentAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut adapter_requests = Vec::new();

        for imp in &request.imp {
            let bidder_val = match imp.ext.as_ref().and_then(|e| e.get("bidder")).cloned() {
                Some(v) => v,
                None => {
                    errs.push(BidderError::BadInput(format!("Impression id={} has an Error: missing bidder ext", imp.id)));
                    continue;
                }
            };
            let deepintent_ext: ExtImpDeepintent = match serde_json::from_value(bidder_val) {
                Ok(e) => e,
                Err(e) => {
                    errs.push(BidderError::BadInput(format!("Impression id={}, has invalid Ext: {}", imp.id, e)));
                    continue;
                }
            };

            let mut req_copy = request.clone();
            let mut imp_copy = imp.clone();
            imp_copy.tagid = Some(deepintent_ext.tag_id.clone());
            imp_copy.displaymanager = Some(DISPLAY_MANAGER.to_string());
            imp_copy.displaymanagerver = Some(DISPLAY_MANAGER_VER.to_string());

            if let Err(e) = build_imp_banner(&mut imp_copy) {
                errs.push(e);
                continue;
            }

            req_copy.imp = vec![imp_copy];

            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(BidderError::BadInput(e.to_string()));
                    continue;
                }
            };

            let mut headers = HashMap::new();
            headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
            headers.insert("Accept".to_string(), "application/json".to_string());

            let imp_ids = get_imp_ids(&req_copy.imp);
            adapter_requests.push(RequestData {
                method: "POST".to_string(),
                uri: self.endpoint.clone(),
                body,
                headers,
                imp_ids,
            });
        }

        (adapter_requests, errs)
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(1);
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                match get_media_type_for_imp(&bid.impid, &internal.imp) {
                    Ok(bid_type) => result.bids.push(TypedBid::new(bid, bid_type)),
                    Err(_) => {} // skip bids for unknown impressions
                }
            }
        }
        Ok(result)
    }
}
