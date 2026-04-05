use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;
use serde_json::Value;

pub struct AdtrgtmeAdapter { pub endpoint: String }
impl AdtrgtmeAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

#[derive(Deserialize)]
struct ExtImpBidder { bidder: Value }

#[derive(Deserialize)]
struct ExtImpAdtrgtme {
    site_id: u64,
}

impl Bidder for AdtrgtmeAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut requests = Vec::new();
        let mut req = request.clone();
        for imp in &request.imp {
            let bidder_ext: ExtImpBidder = match imp.ext.as_ref()
                .and_then(|e| serde_json::from_value(e.clone()).ok()) {
                Some(v) => v,
                None => { errs.push(BidderError::BadInput("ext.bidder not provided".to_string())); continue; }
            };
            let ext: ExtImpAdtrgtme = match serde_json::from_value(bidder_ext.bidder) {
                Ok(v) => v,
                Err(_) => { errs.push(BidderError::BadInput("ext.bidder not provided".to_string())); continue; }
            };
            let mut imp_copy = imp.clone();
            imp_copy.ext = None;
            req.imp = vec![imp_copy];
            let body = match serde_json::to_vec(&req) {
                Ok(b) => b,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };
            let uri = format!("{}?s={}&prebid", self.endpoint, ext.site_id);
            let mut headers = HashMap::new();
            headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
            headers.insert("Accept".to_string(), "application/json".to_string());
            headers.insert("X-Openrtb-Version".to_string(), "2.5".to_string());
            if let Some(device) = &request.device {
                if let Some(ua) = &device.ua { if !ua.is_empty() { headers.insert("User-Agent".to_string(), ua.clone()); } }
                if let Some(ipv6) = &device.ipv6 { if !ipv6.is_empty() { headers.insert("X-Forwarded-For".to_string(), ipv6.clone()); } }
                if let Some(ip) = &device.ip { if !ip.is_empty() { headers.insert("X-Forwarded-For".to_string(), ip.clone()); } }
            }
            requests.push(RequestData { method: "POST".to_string(), uri, body, headers, imp_ids: get_imp_ids(&req.imp) });
        }
        (requests, errs)
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code == 400 {
            return Err(vec![BidderError::BadInput(format!("Unexpected status code: [ {} ]", response.status_code))]);
        }
        if response.status_code == 503 {
            return Err(vec![BidderError::BadInput(format!("Something went wrong, please contact your Account Manager. Status Code: [ {} ] ", response.status_code))]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadInput(format!("Unexpected status code: [ {} ]. Run with request.debug = 1 for more info", response.status_code))]);
        }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|_| vec![BidderError::BadServerResponse("Bad Server Response".to_string())])?;
        if bid_resp.seatbid.is_empty() {
            return Err(vec![BidderError::BadServerResponse("Empty SeatBid array".to_string())]);
        }
        let mut result = BidderResponse::with_capacity(internal.imp.len());
        if let Some(cur) = &bid_resp.cur { result.currency = cur.clone(); }
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let found = internal.imp.iter().find(|i| i.id == bid.impid);
                match found {
                    Some(imp) => {
                        if imp.banner.is_some() {
                            result.bids.push(TypedBid::new(bid, BidType::Banner));
                        } else {
                            return Err(vec![BidderError::BadInput(format!("Unsupported bidtype for bid: \"{}\"", bid.impid))]);
                        }
                    }
                    None => return Err(vec![BidderError::BadInput(format!("Failed to find impression: \"{}\"", bid.impid))]),
                }
            }
        }
        Ok(result)
    }
}
