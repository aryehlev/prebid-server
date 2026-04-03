use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;
use serde_json::Value;

pub struct BmtmAdapter {
    pub endpoint: String,
}

impl BmtmAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

#[derive(Deserialize)]
struct ExtImpBidder {
    bidder: Value,
}

#[derive(Deserialize)]
struct ImpExtBmtm {
    #[serde(rename = "placement_id")]
    placement_id: i32,
}

fn get_media_type_for_bid(imp_id: &str, imps: &[openrtb::Imp]) -> BidType {
    for imp in imps {
        if imp.id == imp_id {
            if imp.banner.is_some() { return BidType::Banner; }
            if imp.video.is_some() { return BidType::Video; }
        }
    }
    BidType::Banner
}

impl Bidder for BmtmAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut results = Vec::new();
        let mut errs = Vec::new();

        for imp in &request.imp {
            if imp.banner.is_none() && imp.video.is_none() {
                errs.push(BidderError::BadInput(format!(
                    "For Imp ID {} Banner or Video is undefined", imp.id
                )));
                continue;
            }

            let ext_val = match &imp.ext {
                Some(v) => v.clone(),
                None => {
                    errs.push(BidderError::BadInput(format!("Error unmarshalling ExtImpBidder: missing ext")));
                    continue;
                }
            };

            let bidder_ext: ExtImpBidder = match serde_json::from_value(ext_val) {
                Ok(v) => v,
                Err(e) => {
                    errs.push(BidderError::BadInput(format!("Error unmarshalling ExtImpBidder: {}", e)));
                    continue;
                }
            };

            let bmtm_ext: ImpExtBmtm = match serde_json::from_value(bidder_ext.bidder) {
                Ok(v) => v,
                Err(e) => {
                    errs.push(BidderError::BadInput(format!("Error unmarshalling ExtImpBmtm: {}", e)));
                    continue;
                }
            };

            let mut imp_copy = imp.clone();
            imp_copy.tagid = Some(bmtm_ext.placement_id.to_string());
            imp_copy.ext = None;

            let mut req_copy = request.clone();
            req_copy.imp = vec![imp_copy];

            // Build headers
            let mut headers = HashMap::new();
            headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
            headers.insert("Accept".to_string(), "application/json".to_string());

            if let Some(device) = &request.device {
                if let Some(ua) = &device.ua {
                    if !ua.is_empty() {
                        headers.insert("User-Agent".to_string(), ua.clone());
                    }
                }
                if let Some(ip) = &device.ip {
                    if !ip.is_empty() {
                        headers.insert("X-Forwarded-For".to_string(), ip.clone());
                    }
                } else if let Some(ipv6) = &device.ipv6 {
                    if !ipv6.is_empty() {
                        headers.insert("X-Forwarded-For".to_string(), ipv6.clone());
                    }
                }
            }
            if let Some(site) = &request.site {
                if let Some(page) = &site.page {
                    if !page.is_empty() {
                        headers.insert("Referer".to_string(), page.clone());
                    }
                }
            }

            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(BidderError::BadInput(e.to_string()));
                    continue;
                }
            };

            results.push(RequestData {
                method: "POST".to_string(),
                uri: self.endpoint.clone(),
                body,
                headers,
                imp_ids: get_imp_ids(&req_copy.imp),
            });
        }

        (results, errs)
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code == 400 {
            return Err(vec![BidderError::BadInput(format!(
                "Unknown status code: {}.", response.status_code
            ))]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unknown status code: {}.", response.status_code
            ))]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(1);
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = get_media_type_for_bid(&bid.impid, &internal.imp);
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }

        Ok(result)
    }
}
