use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;
use serde_json::Value;

pub struct BidmachineAdapter {
    pub endpoint: String,
}

impl BidmachineAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

#[derive(Deserialize)]
struct ExtImpBidder {
    bidder: Value,
}

#[derive(Deserialize, Default)]
struct ExtImpBidmachine {
    #[serde(rename = "host", default)]
    host: String,
    #[serde(rename = "path", default)]
    path: String,
    #[serde(rename = "seller_id", default)]
    seller_id: String,
}

fn get_media_type_for_imp(imp_id: &str, imps: &[openrtb::Imp]) -> Option<BidType> {
    for imp in imps {
        if imp.id == imp_id {
            // Default is banner; only switch to video if banner is nil and video is present
            if imp.banner.is_none() && imp.video.is_some() {
                return Some(BidType::Video);
            }
            return Some(BidType::Banner);
        }
    }
    None
}

impl Bidder for BidmachineAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
        headers.insert("X-Openrtb-Version".to_string(), "2.5".to_string());

        let mut results = Vec::new();
        let mut errs = Vec::new();

        for imp in &request.imp {
            // Validate banner dimensions
            if let Some(banner) = &imp.banner {
                if banner.w.is_none() && banner.h.is_none() {
                    match &banner.format {
                        None => {
                            errs.push(BidderError::BadInput(format!(
                                "Impression with id: {} has following error: Banner width and height is not provided and banner format is missing. At least one is required",
                                imp.id
                            )));
                            continue;
                        }
                        Some(formats) if formats.is_empty() => {
                            errs.push(BidderError::BadInput(format!(
                                "Impression with id: {} has following error: Banner width and height is not provided and banner format array is empty. At least one is required",
                                imp.id
                            )));
                            continue;
                        }
                        _ => {}
                    }
                }
            }

            let ext_val = match &imp.ext {
                Some(v) => v.clone(),
                None => { errs.push(BidderError::BadInput("missing imp ext".to_string())); continue; }
            };

            let bidder_ext: ExtImpBidder = match serde_json::from_value(ext_val) {
                Ok(v) => v,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };

            let bm_ext: ExtImpBidmachine = match serde_json::from_value(bidder_ext.bidder) {
                Ok(v) => v,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };

            // Build URL: replace {{.Host}} with host, then append path and seller_id
            let base_url = self.endpoint.replace("{{.Host}}", &bm_ext.host);
            let url = format!("{}/{}/{}", base_url.trim_end_matches('/'),
                bm_ext.path.trim_matches('/'),
                bm_ext.seller_id);

            let mut req_copy = request.clone();
            req_copy.imp = vec![imp.clone()];

            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };

            results.push(RequestData {
                method: "POST".to_string(),
                uri: url,
                body,
                headers: headers.clone(),
                imp_ids: get_imp_ids(&req_copy.imp),
            });
        }

        (results, errs)
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        match response.status_code {
            204 => return Ok(BidderResponse::new()),
            400 | 401 | 403 | 503 => {
                return Err(vec![BidderError::BadInput(format!(
                    "unexpected status code: {} {}", response.status_code,
                    String::from_utf8_lossy(&response.body)
                ))]);
            }
            200 => {}
            _ => {
                return Err(vec![BidderError::BadServerResponse(format!(
                    "unexpected status code: {} {}", response.status_code,
                    String::from_utf8_lossy(&response.body)
                ))]);
            }
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(internal.imp.len());
        let mut errs = Vec::new();

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                match get_media_type_for_imp(&bid.impid, &internal.imp) {
                    Some(bt) => result.bids.push(TypedBid::new(bid, bt)),
                    None => errs.push(BidderError::BadServerResponse(format!(
                        "ignoring bid id={}, request doesn't contain any valid impression with id={}",
                        bid.id, bid.impid
                    ))),
                }
            }
        }

        Ok(result)
    }
}
