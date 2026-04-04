use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use openrtb_ext::BidType;

pub struct SilvermobAdapter { pub endpoint: String }
impl SilvermobAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

fn get_imp_ext_host_zone(imp: &openrtb::Imp) -> Option<(String, String)> {
    let bidder = imp.ext.as_ref()?.get("bidder")?;
    let host = bidder.get("host").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let zone_id = bidder.get("zoneId").and_then(|v| v.as_str()).unwrap_or("").to_string();
    Some((host, zone_id))
}

fn build_url(endpoint: &str, host: &str, zone_id: &str) -> String {
    endpoint
        .replace("{{.Host}}", host)
        .replace("{{.ZoneID}}", zone_id)
}

fn get_bid_media_type_from_mtype(bid: &openrtb::Bid) -> Result<BidType, BidderError> {
    match bid.mtype.unwrap_or(0) {
        1 => Ok(BidType::Banner),
        2 => Ok(BidType::Video),
        4 => Ok(BidType::Native),
        _ => Err(BidderError::BadServerResponse(format!("Unable to fetch mediaType for imp: {}", bid.impid))),
    }
}

impl Bidder for SilvermobAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();
        let mut errs = Vec::new();
        for imp in &request.imp {
            let (host, zone_id) = match get_imp_ext_host_zone(imp) {
                Some(p) => p,
                None => { errs.push(BidderError::BadInput("missing host/zoneId in imp.ext.bidder".to_string())); continue; }
            };
            let valid_hosts = ["eu", "us", "apac", "global"];
            if !valid_hosts.contains(&host.as_str()) {
                errs.push(BidderError::BadInput(format!("invalid host {}", host)));
                continue;
            }
            let url = build_url(&self.endpoint, &host, &zone_id);
            let mut req_copy = request.clone();
            req_copy.imp = vec![imp.clone()];
            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };
            let mut headers = HashMap::new();
            headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
            headers.insert("Accept".to_string(), "application/json".to_string());
            headers.insert("X-Openrtb-Version".to_string(), "2.5".to_string());
            if let Some(device) = &request.device {
                if let Some(ua) = &device.ua { if !ua.is_empty() { headers.insert("User-Agent".to_string(), ua.clone()); } }
                if let Some(ipv6) = &device.ipv6 { if !ipv6.is_empty() { headers.insert("X-Forwarded-For".to_string(), ipv6.clone()); } }
                if let Some(ip) = &device.ip { if !ip.is_empty() { headers.insert("X-Forwarded-For".to_string(), ip.clone()); } }
            }
            let imp_ids = vec![imp.id.clone()];
            requests.push(RequestData { method: "POST".to_string(), uri: url, body, headers, imp_ids });
        }
        (requests, errs)
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(format!("Error unmarshaling server Response: {}", e))])?;
        if bid_resp.seatbid.is_empty() {
            return Err(vec![BidderError::BadServerResponse("Empty SeatBid array".to_string())]);
        }
        let mut result = BidderResponse::with_capacity(1);
        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                match get_bid_media_type_from_mtype(&bid) {
                    Ok(bid_type) => result.bids.push(TypedBid::new(bid, bid_type)),
                    Err(e) => errs.push(e),
                }
            }
        }
        if !errs.is_empty() && result.bids.is_empty() { return Err(errs); }
        Ok(result)
    }
}
