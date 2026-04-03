use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use openrtb_ext::BidType;
use serde::Deserialize;

pub struct PubnativeAdapter { pub endpoint: String }
impl PubnativeAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

#[derive(Debug, Deserialize, Default)]
struct ExtImpPubnative {
    #[serde(rename = "zone_id", default)]
    zone_id: i64,
    #[serde(rename = "app_auth_token", default)]
    app_auth_token: String,
}

fn get_media_type_for_imp(imp_id: &str, imps: &[openrtb::Imp]) -> BidType {
    for imp in imps {
        if imp.id == imp_id {
            if imp.video.is_some() { return BidType::Video; }
            if imp.native.is_some() { return BidType::Native; }
            return BidType::Banner;
        }
    }
    BidType::Banner
}

fn convert_banner(banner: &mut openrtb::Banner) -> Result<(), BidderError> {
    let w_ok = banner.w.map_or(false, |w| w != 0);
    let h_ok = banner.h.map_or(false, |h| h != 0);
    if !w_ok || !h_ok {
        if let Some(formats) = &banner.format {
            if !formats.is_empty() {
                let f = formats[0].clone();
                banner.w = f.w;
                banner.h = f.h;
                return Ok(());
            }
        }
        return Err(BidderError::BadInput("Size information missing for banner".to_string()));
    }
    Ok(())
}

impl Bidder for PubnativeAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        // Check device OS
        if request.device.as_ref().and_then(|d| d.os.as_ref()).map(|s| s.is_empty()).unwrap_or(true) {
            return (vec![], vec![BidderError::BadInput("Impression is missing device OS information".to_string())]);
        }

        let mut requests = Vec::new();
        let mut errs = Vec::new();
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        for imp in &request.imp {
            if imp.banner.is_none() && imp.video.is_none() && imp.native.is_none() {
                errs.push(BidderError::BadInput("Pubnative only supports banner, video or native ads.".to_string()));
                continue;
            }

            let bidder_val = match imp.ext.as_ref().and_then(|e| e.get("bidder")) {
                Some(v) => v.clone(),
                None => {
                    errs.push(BidderError::BadInput("missing bidder ext".to_string()));
                    continue;
                }
            };
            let pn_ext: ExtImpPubnative = match serde_json::from_value(bidder_val) {
                Ok(e) => e,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };

            let mut imp_copy = imp.clone();
            // Convert banner size if needed
            if let Some(banner) = imp_copy.banner.as_mut() {
                if let Err(e) = convert_banner(banner) {
                    errs.push(e);
                    continue;
                }
            }

            let uri = format!("{}?apptoken={}&zoneid={}", self.endpoint, pn_ext.app_auth_token, pn_ext.zone_id);
            let mut req_copy = request.clone();
            req_copy.imp = vec![imp_copy];
            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); return (vec![], errs); }
            };
            requests.push(RequestData {
                method: "POST".to_string(),
                uri,
                body,
                headers: headers.clone(),
                imp_ids: vec![imp.id.clone()],
            });
        }
        (requests, errs)
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code == 400 {
            return Err(vec![BidderError::BadInput(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info",
                response.status_code
            ))]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info",
                response.status_code
            ))]);
        }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(1);
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                if bid.price != 0.0 {
                    let bid_type = get_media_type_for_imp(&bid.impid, &internal.imp);
                    result.bids.push(TypedBid::new(bid, bid_type));
                }
            }
        }
        Ok(result)
    }
}
