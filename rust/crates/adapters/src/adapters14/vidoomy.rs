use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct VidoomyAdapter { pub endpoint: String }
impl VidoomyAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

fn get_imp_info(imp_id: &str, imps: &[openrtb::Imp]) -> Option<BidType> {
    for imp in imps {
        if imp.id == imp_id {
            if imp.video.is_some() { return Some(BidType::Video); }
            if imp.banner.is_some() { return Some(BidType::Banner); }
        }
    }
    None
}

fn get_headers(request: &openrtb::BidRequest) -> HashMap<String, String> {
    let mut h = HashMap::new();
    h.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
    h.insert("Accept".to_string(), "application/json".to_string());
    h.insert("x-openrtb-version".to_string(), "2.5".to_string());
    if let Some(device) = &request.device {
        if let Some(ua) = &device.ua {
            if !ua.is_empty() {
                h.insert("User-Agent".to_string(), ua.clone());
            }
        }
        if let Some(ipv6) = &device.ipv6 {
            if !ipv6.is_empty() {
                h.insert("X-Forwarded-For".to_string(), ipv6.clone());
            }
        }
        if let Some(ip) = &device.ip {
            if !ip.is_empty() {
                h.insert("X-Forwarded-For".to_string(), ip.clone());
            }
        }
    }
    h
}

fn change_request_for_bid_service(imp: &mut openrtb::Imp) -> Result<(), BidderError> {
    if let Some(banner) = imp.banner.as_mut() {
        // If W and H are explicitly set, validate them
        if banner.w.is_some() && banner.h.is_some() {
            let w = banner.w.unwrap_or(0);
            let h = banner.h.unwrap_or(0);
            if w == 0 || h == 0 {
                return Err(BidderError::BadInput(format!("invalid sizes provided for Banner {} x {}", w, h)));
            }
            return Ok(());
        }
        // Otherwise use first format entry
        let formats = banner.format.as_ref().filter(|f| !f.is_empty());
        if formats.is_none() {
            return Err(BidderError::BadInput(format!("no sizes provided for Banner {:?}", banner.format)));
        }
        let first = &formats.unwrap()[0];
        banner.w = first.w;
        banner.h = first.h;
    }
    Ok(())
}

impl Bidder for VidoomyAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();
        let mut errs = Vec::new();
        let headers = get_headers(request);
        for imp in &request.imp {
            let mut req_copy = request.clone();
            req_copy.imp = vec![imp.clone()];
            if let Err(e) = change_request_for_bid_service(&mut req_copy.imp[0]) {
                errs.push(e);
                continue;
            }
            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };
            requests.push(RequestData { method: "POST".to_string(), uri: self.endpoint.clone(), body, headers: headers.clone(), imp_ids: vec![imp.id.clone()] });
        }
        (requests, errs)
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!("Unexpected status code: {}.", response.status_code))]);
        }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(format!("Bad server response: {}.", e))])?;
        let mut result = BidderResponse::with_capacity(internal.imp.len());
        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                match get_imp_info(&bid.impid, &internal.imp) {
                    Some(bid_type) => {
                        if bid_type == BidType::Banner || bid_type == BidType::Video {
                            result.bids.push(TypedBid::new(bid, bid_type));
                        }
                        // other types are silently ignored per Go code
                    }
                    None => {
                        errs.push(BidderError::BadServerResponse(format!("Unknown ad unit code '{}'", bid.impid)));
                    }
                }
            }
        }
        if !errs.is_empty() && result.bids.is_empty() { return Err(errs); }
        Ok(result)
    }
}
