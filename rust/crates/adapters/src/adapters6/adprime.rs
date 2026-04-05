use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;
use serde_json::Value;

pub struct AdprimeAdapter { pub endpoint: String }
impl AdprimeAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

#[derive(Deserialize, Default)]
struct ExtImpBidder { bidder: Value }

#[derive(Deserialize, Default)]
struct ExtImpAdprime {
    #[serde(rename = "TagID", default)]
    tag_id: String,
    #[serde(default)]
    keywords: Vec<String>,
    #[serde(default)]
    audiences: Vec<String>,
}

fn bid_type_from_mtype(mtype: Option<i32>) -> Result<BidType, String> {
    match mtype {
        Some(1) => Ok(BidType::Banner),
        Some(2) => Ok(BidType::Video),
        Some(4) => Ok(BidType::Native),
        _ => Err(format!("unknown mtype: {:?}", mtype)),
    }
}

impl Bidder for AdprimeAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut requests = Vec::new();
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        for imp in &request.imp {
            let bidder_ext: ExtImpBidder = match imp.ext.as_ref()
                .and_then(|e| serde_json::from_value(e.clone()).ok()) {
                Some(v) => v,
                None => { errs.push(BidderError::BadInput("failed to parse ext".to_string())); return (vec![], errs); }
            };
            let adprime_ext: ExtImpAdprime = match serde_json::from_value(bidder_ext.bidder) {
                Ok(v) => v,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); return (vec![], errs); }
            };

            let mut req = request.clone();
            let mut imp_copy = imp.clone();
            imp_copy.tagid = Some(adprime_ext.tag_id.clone());
            // Build new ext with bidder info
            let new_ext = serde_json::json!({
                "bidder": { "TagID": &adprime_ext.tag_id, "placementId": &adprime_ext.tag_id }
            });
            imp_copy.ext = Some(new_ext);
            req.imp = vec![imp_copy];

            if let Some(site) = &req.site {
                if !adprime_ext.keywords.is_empty() {
                    let mut site_copy = site.clone();
                    site_copy.keywords = Some(adprime_ext.keywords.join(","));
                    req.site = Some(site_copy);
                }
            }
            if !adprime_ext.audiences.is_empty() {
                let user = req.user.get_or_insert_with(Default::default);
                user.customdata = Some(adprime_ext.audiences.join(","));
            }

            let body = match serde_json::to_vec(&req) {
                Ok(b) => b,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };
            requests.push(RequestData { method: "POST".to_string(), uri: self.endpoint.clone(), body, headers: headers.clone(), imp_ids: get_imp_ids(&req.imp) });
        }
        (requests, errs)
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code == 404 {
            return Err(vec![BidderError::BadServerResponse(format!("Page not found: {}.", response.status_code))]);
        }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(5);
        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                match bid_type_from_mtype(bid.mtype) {
                    Ok(bt) => result.bids.push(TypedBid::new(bid, bt)),
                    Err(e) => errs.push(BidderError::BadServerResponse(format!("Unable to fetch mediaType in multi-format: {} ({})", bid.impid, e))),
                }
            }
        }
        Ok(result)
    }
}
