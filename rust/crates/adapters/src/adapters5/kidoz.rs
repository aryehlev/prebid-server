use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct KidozAdapter { pub endpoint: String }
impl KidozAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

#[derive(serde::Deserialize, Default)]
struct KidozImpExt {
    #[serde(rename = "access_token", default)]
    access_token: String,
    #[serde(rename = "publisher_id", default)]
    publisher_id: String,
}

fn get_media_type_for_imp(imp_id: &str, imps: &[openrtb::Imp]) -> Option<BidType> {
    for imp in imps {
        if imp.id == imp_id {
            if imp.banner.is_some() { return Some(BidType::Banner); }
            if imp.video.is_some() { return Some(BidType::Video); }
            if imp.native.is_some() { return Some(BidType::Native); }
            if imp.audio.is_some() { return Some(BidType::Audio); }
        }
    }
    None
}

impl Bidder for KidozAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();
        let mut errs = Vec::new();

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
        headers.insert("x-openrtb-version".to_string(), "2.5".to_string());

        for imp in &request.imp {
            // Validate imp type
            if imp.banner.is_none() && imp.video.is_none() {
                errs.push(BidderError::BadInput("Kidoz only supports banner or video ads".to_string()));
                continue;
            }

            if let Some(banner) = &imp.banner {
                let formats = banner.format.as_deref().unwrap_or(&[]);
                if formats.is_empty() {
                    errs.push(BidderError::BadInput("banner format required".to_string()));
                    continue;
                }
            }

            // Extract bidder ext
            let ext = match imp.ext.as_ref() {
                Some(e) => e,
                None => {
                    errs.push(BidderError::BadInput("impression extensions required".to_string()));
                    continue;
                }
            };

            let bidder_val = match ext.get("bidder") {
                Some(v) => v.clone(),
                None => {
                    errs.push(BidderError::BadInput("bidder required".to_string()));
                    continue;
                }
            };

            let kidoz_ext: KidozImpExt = match serde_json::from_value(bidder_val) {
                Ok(v) => v,
                Err(e) => {
                    errs.push(BidderError::BadInput(e.to_string()));
                    continue;
                }
            };

            if kidoz_ext.access_token.is_empty() {
                errs.push(BidderError::BadInput("Kidoz access_token required".to_string()));
                continue;
            }
            if kidoz_ext.publisher_id.is_empty() {
                errs.push(BidderError::BadInput("Kidoz publisher_id required".to_string()));
                continue;
            }

            // Build per-imp request
            let mut req_copy = request.clone();
            req_copy.imp = vec![imp.clone()];

            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(BidderError::BadInput(e.to_string()));
                    continue;
                }
            };

            requests.push(RequestData {
                method: "POST".to_string(),
                uri: self.endpoint.clone(),
                body,
                headers: headers.clone(),
                imp_ids: get_imp_ids(&req_copy.imp),
            });
        }

        if requests.is_empty() {
            return (vec![], errs);
        }
        (requests, errs)
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        match response.status_code {
            204 | 503 => return Ok(BidderResponse::new()),
            400 | 401 | 403 => return Err(vec![BidderError::BadInput(format!(
                "unexpected status code: {} {}",
                response.status_code,
                String::from_utf8_lossy(&response.body)
            ))]),
            200 => {}
            _ => return Err(vec![BidderError::BadServerResponse(format!(
                "unexpected status code: {} {}",
                response.status_code,
                String::from_utf8_lossy(&response.body)
            ))]),
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(internal.imp.len());
        let mut errs = Vec::new();

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                match get_media_type_for_imp(&bid.impid, &internal.imp) {
                    Some(bid_type) => result.bids.push(TypedBid::new(bid, bid_type)),
                    None => errs.push(BidderError::BadServerResponse(format!(
                        "ignoring bid id={}, request doesn't contain any valid impression with id={}",
                        bid.id, bid.impid
                    ))),
                }
            }
        }

        if !errs.is_empty() && result.bids.is_empty() {
            return Err(errs);
        }
        Ok(result)
    }
}
