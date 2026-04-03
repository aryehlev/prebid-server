use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;
use serde_json::Value;

pub struct BeintooAdapter {
    pub endpoint: String,
}

impl BeintooAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

#[derive(Deserialize)]
struct ExtImpBidder {
    bidder: Value,
}

#[derive(Deserialize)]
struct ExtImpBeintoo {
    #[serde(rename = "tagid", default)]
    tag_id: String,
    #[serde(rename = "bidfloor", default)]
    bid_floor: String,
}

impl Bidder for BeintooAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::BadInput("No Imps in Bid Request".to_string())]);
        }

        // Determine secure flag from site.page URL
        let secure = if let Some(site) = &request.site {
            if let Some(page) = &site.page {
                if page.starts_with("https://") { 1i32 } else { 0i32 }
            } else { 0i32 }
        } else { 0i32 };

        let mut req_copy = request.clone();
        let mut processed_imps = Vec::with_capacity(request.imp.len());

        for imp in &request.imp {
            let ext_val = match &imp.ext {
                Some(v) => v.clone(),
                None => return (vec![], vec![BidderError::BadInput(format!("missing imp ext for imp {}", imp.id))]),
            };

            let bidder_ext: ExtImpBidder = match serde_json::from_value(ext_val) {
                Ok(v) => v,
                Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
            };

            let beintoo_ext: ExtImpBeintoo = match serde_json::from_value(bidder_ext.bidder) {
                Ok(v) => v,
                Err(e) => return (vec![], vec![BidderError::BadInput(format!("ignoring imp id={}, invalid ImpExt", imp.id))]),
            };

            // Validate tagid is numeric non-zero
            match beintoo_ext.tag_id.parse::<i64>() {
                Ok(v) if v != 0 => {}
                _ => return (vec![], vec![BidderError::BadInput(format!(
                    "ignoring imp id={}, invalid tagid must be a String of numbers", imp.id
                ))]),
            }

            let mut imp_copy = imp.clone();
            imp_copy.tagid = Some(beintoo_ext.tag_id);
            imp_copy.secure = Some(secure);
            imp_copy.ext = None;

            // Set bid floor
            if !beintoo_ext.bid_floor.is_empty() {
                if let Ok(floor) = beintoo_ext.bid_floor.parse::<f64>() {
                    if floor > 0.0 {
                        imp_copy.bidfloor = Some(floor);
                    }
                }
            }

            // Process banner: set W/H from first format if not set
            if let Some(banner) = &imp_copy.banner {
                if banner.w.is_none() && banner.h.is_none() {
                    if banner.format.as_deref().unwrap_or(&[]).is_empty() {
                        return (vec![], vec![BidderError::BadInput("Need at least one size to build request".to_string())]);
                    }
                    let mut banner_copy = banner.clone();
                    let formats = banner_copy.format.as_mut().unwrap();
                    if formats.is_empty() {
                        return (vec![], vec![BidderError::BadInput("Need at least one size to build request".to_string())]);
                    }
                    let first = formats.remove(0);
                    banner_copy.w = first.w;
                    banner_copy.h = first.h;
                    imp_copy.banner = Some(banner_copy);
                }
            } else {
                return (vec![], vec![BidderError::BadInput("Request needs to include a Banner object".to_string())]);
            }

            processed_imps.push(imp_copy);
        }

        req_copy.imp = processed_imps;

        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput("Error in packaging request to JSON".to_string())]),
        };

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
            }
            if let Some(lang) = &device.language {
                if !lang.is_empty() {
                    headers.insert("Accept-Language".to_string(), lang.clone());
                }
            }
            if let Some(dnt) = &device.dnt {
                headers.insert("DNT".to_string(), dnt.to_string());
            }
        }
        if let Some(site) = &request.site {
            if let Some(page) = &site.page {
                if !page.is_empty() {
                    headers.insert("Referer".to_string(), page.clone());
                }
            }
        }

        (vec![RequestData {
            method: "POST".to_string(),
            uri: self.endpoint.clone(),
            body,
            headers,
            imp_ids: get_imp_ids(&req_copy.imp),
        }], vec![])
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Invalid Status Returned: {}. Run with request.debug = 1 for more info", response.status_code
            ))]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(format!(
                "Unable to unpackage bid response. Error: {}", e
            ))])?;

        let mut result = BidderResponse::with_capacity(1);
        for sb in bid_resp.seatbid {
            for mut bid in sb.bid {
                bid.impid = bid.id.clone();
                result.bids.push(TypedBid::new(bid, BidType::Banner));
            }
        }
        Ok(result)
    }
}
