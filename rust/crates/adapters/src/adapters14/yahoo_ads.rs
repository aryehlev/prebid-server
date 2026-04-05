use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use openrtb_ext::BidType;
use serde::Deserialize;

pub struct YahooAdsAdapter { pub endpoint: String }
impl YahooAdsAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

#[derive(Debug, Deserialize, Default)]
struct ExtImpYahooAds {
    #[serde(rename = "dcn", default)]
    dcn: String,
    #[serde(rename = "pos", default)]
    pos: String,
}

fn validate_banner(banner: &mut openrtb::Banner) -> Result<(), BidderError> {
    if let (Some(w), Some(h)) = (banner.w, banner.h) {
        if w == 0 || h == 0 {
            return Err(BidderError::BadInput(format!("Invalid sizes provided for Banner {}x{}", w, h)));
        }
        return Ok(());
    }
    if banner.format.as_deref().unwrap_or(&[]).is_empty() {
        return Err(BidderError::BadInput("No sizes provided for Banner".to_string()));
    }
    let first = banner.format.as_ref().unwrap()[0].clone();
    banner.w = first.w;
    banner.h = first.h;
    Ok(())
}

fn get_imp_info(imp_id: &str, imps: &[openrtb::Imp]) -> (bool, BidType) {
    for imp in imps {
        if imp.id == imp_id {
            let media_type = if imp.banner.is_some() {
                BidType::Banner
            } else if imp.video.is_some() {
                BidType::Video
            } else {
                BidType::Banner
            };
            return (true, media_type);
        }
    }
    (false, BidType::Banner)
}

impl Bidder for YahooAdsAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut requests = Vec::new();

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
        headers.insert("x-openrtb-version".to_string(), "2.5".to_string());

        if let Some(device) = &request.device {
            if let Some(ua) = &device.ua {
                if !ua.is_empty() {
                    headers.insert("User-Agent".to_string(), ua.clone());
                }
            }
        }

        for (idx, imp) in request.imp.iter().enumerate() {
            let bidder_val = match imp.ext.as_ref().and_then(|e| e.get("bidder")) {
                Some(v) => v.clone(),
                None => {
                    errs.push(BidderError::BadInput(format!("imp #{}: ext.bidder not provided", idx)));
                    continue;
                }
            };
            let yahoo_ext: ExtImpYahooAds = match serde_json::from_value(bidder_val) {
                Ok(e) => e,
                Err(e) => {
                    errs.push(BidderError::BadInput(format!("imp #{}: {}", idx, e)));
                    continue;
                }
            };

            let mut req_copy = request.clone();
            req_copy.imp = vec![imp.clone()];

            req_copy.imp[0].tagid = Some(yahoo_ext.pos.clone());

            if let Some(site) = req_copy.site.as_mut() {
                site.id = Some(yahoo_ext.dcn.clone());
            } else if let Some(app) = req_copy.app.as_mut() {
                app.id = Some(yahoo_ext.dcn.clone());
            }

            if let Some(banner) = req_copy.imp[0].banner.as_mut() {
                if let Err(e) = validate_banner(banner) {
                    errs.push(e);
                    continue;
                }
            }

            let body = match serde_json::to_vec(&req_copy) {
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

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected status code: {}.", response.status_code
            ))]);
        }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(format!("Bad server response: {}.", e))])?;
        let mut result = BidderResponse::with_capacity(internal.imp.len());
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let (exists, media_type) = get_imp_info(&bid.impid, &internal.imp);
                if !exists {
                    return Err(vec![BidderError::BadServerResponse(format!(
                        "Unknown ad unit code '{}'", bid.impid
                    ))]);
                }
                if media_type != BidType::Banner && media_type != BidType::Video {
                    continue;
                }
                result.bids.push(TypedBid::new(bid, media_type));
            }
        }
        Ok(result)
    }
}
