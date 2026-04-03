use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;

pub struct TheadxAdapter { pub endpoint: String }
impl TheadxAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

#[derive(Debug, Deserialize)]
struct ExtImpTheadx {
    #[serde(rename = "tagId")]
    tag_id: serde_json::Value,
}

fn get_tag_id_string(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        _ => v.to_string(),
    }
}

fn get_media_type_for_bid(bid: &openrtb::Bid) -> Result<BidType, BidderError> {
    if let Some(ext) = &bid.ext {
        if let Some(prebid) = ext.get("prebid") {
            if let Some(type_str) = prebid.get("type").and_then(|v| v.as_str()) {
                return match type_str {
                    "banner" => Ok(BidType::Banner),
                    "video" => Ok(BidType::Video),
                    "native" => Ok(BidType::Native),
                    "audio" => Ok(BidType::Audio),
                    _ => Err(BidderError::BadServerResponse(format!(
                        "Failed to parse impression \"{}\" mediatype", bid.impid
                    ))),
                };
            }
        }
    }
    Err(BidderError::BadServerResponse(format!(
        "Failed to parse impression \"{}\" mediatype", bid.impid
    )))
}

impl Bidder for TheadxAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut req_copy = request.clone();
        let mut errs = Vec::new();
        let mut valid_imps = Vec::new();

        for mut imp in req_copy.imp.clone() {
            let bidder_val = match imp.ext.as_ref().and_then(|e| e.get("bidder")) {
                Some(v) => v.clone(),
                None => {
                    errs.push(BidderError::BadInput("missing bidder ext".to_string()));
                    continue;
                }
            };
            let theadx_ext: ExtImpTheadx = match serde_json::from_value(bidder_val) {
                Ok(e) => e,
                Err(e) => {
                    errs.push(BidderError::BadInput(e.to_string()));
                    continue;
                }
            };
            imp.tagid = Some(get_tag_id_string(&theadx_ext.tag_id));
            valid_imps.push(imp);
        }

        req_copy.imp = valid_imps;

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
        headers.insert("X-Openrtb-Version".to_string(), "2.5".to_string());
        headers.insert("X-TEST".to_string(), "1".to_string());

        if let Some(device) = &request.device {
            if let Some(ua) = &device.ua {
                if !ua.is_empty() {
                    headers.insert("X-Device-User-Agent".to_string(), ua.clone());
                }
            }
            if let Some(ipv6) = &device.ipv6 {
                if !ipv6.is_empty() {
                    headers.insert("X-Real-IP".to_string(), ipv6.clone());
                }
            }
            if let Some(ip) = &device.ip {
                if !ip.is_empty() {
                    headers.insert("X-Real-IP".to_string(), ip.clone());
                }
            }
        }

        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => return (vec![], { errs.push(BidderError::BadInput(e.to_string())); errs }),
        };

        let imp_ids = get_imp_ids(&req_copy.imp);
        (vec![RequestData {
            method: "POST".to_string(),
            uri: self.endpoint.clone(),
            body,
            headers,
            imp_ids,
        }], errs)
    }

    fn make_bids(&self, request: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code == 400 {
            return Err(vec![BidderError::BadInput(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info", response.status_code
            ))]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected status code: {}.", response.status_code
            ))]);
        }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(request.imp.len());
        if let Some(cur) = &bid_resp.cur {
            result.currency = cur.clone();
        }
        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                match get_media_type_for_bid(&bid) {
                    Ok(t) => result.bids.push(TypedBid::new(bid, t)),
                    Err(e) => errs.push(e),
                }
            }
        }
        if !errs.is_empty() { return Err(errs); }
        Ok(result)
    }
}
