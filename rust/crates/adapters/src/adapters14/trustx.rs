use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::{BidType, ExtBidPrebidMeta, ExtBidPrebidVideo};

pub struct TrustxAdapter { pub endpoint: String }
impl TrustxAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

fn get_headers(request: &openrtb::BidRequest) -> HashMap<String, String> {
    let mut h = HashMap::new();
    h.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
    h.insert("Accept".to_string(), "application/json".to_string());
    h.insert("X-Openrtb-Version".to_string(), "2.6".to_string());
    if let Some(site) = &request.site {
        if let Some(r) = &site.ref_ { if !r.is_empty() { h.insert("Referer".to_string(), r.clone()); } }
        if let Some(d) = &site.domain { if !d.is_empty() { h.insert("Origin".to_string(), d.clone()); } }
    }
    if let Some(device) = &request.device {
        if let Some(ip) = &device.ip { if !ip.is_empty() { h.insert("X-Forwarded-For".to_string(), ip.clone()); } }
        if let Some(ip6) = &device.ipv6 { if !ip6.is_empty() { h.insert("X-Forwarded-For".to_string(), ip6.clone()); } }
        if let Some(ua) = &device.ua { if !ua.is_empty() { h.insert("User-Agent".to_string(), ua.clone()); } }
    }
    h
}

fn set_imp_ext_gpid(imp: &mut openrtb::Imp) {
    let ad_slot = imp.ext.as_ref()
        .and_then(|ext| ext.get("data"))
        .and_then(|d| d.get("adserver"))
        .and_then(|a| a.get("adslot"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    if let Some(slot) = ad_slot {
        if !slot.is_empty() {
            if let Some(ext_mut) = imp.ext.as_mut() {
                if let Some(obj) = ext_mut.as_object_mut() {
                    obj.insert("gpid".to_string(), serde_json::Value::String(slot));
                }
            }
        }
    }
}

/// Extract network name from bid ext for bid meta.
fn get_bid_meta(bid: &openrtb::Bid) -> Option<ExtBidPrebidMeta> {
    let network_name = bid.ext.as_ref()
        .and_then(|e| e.get("bidder"))
        .and_then(|b| b.get("trustx"))
        .and_then(|t| t.get("networkName"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())?;
    if network_name.is_empty() {
        return None;
    }
    Some(ExtBidPrebidMeta {
        network_name: Some(network_name),
        ..Default::default()
    })
}

/// Build ExtBidPrebidVideo for video bids.
fn get_bid_video(bid: &openrtb::Bid) -> ExtBidPrebidVideo {
    let primary_category = bid.cat.as_ref()
        .and_then(|c| c.first())
        .cloned()
        .unwrap_or_default();
    ExtBidPrebidVideo {
        duration: 0,
        primary_category,
    }
}

impl Bidder for TrustxAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut req_copy = request.clone();
        let mut errs = Vec::new();
        for imp in &mut req_copy.imp {
            set_imp_ext_gpid(imp);
        }
        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => { errs.push(BidderError::BadInput(e.to_string())); return (vec![], errs); }
        };
        let headers = get_headers(request);
        (vec![RequestData { method: "POST".to_string(), uri: self.endpoint.clone(), body, headers, imp_ids: get_imp_ids(&request.imp) }], vec![])
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|_| vec![BidderError::BadServerResponse("Bad Server Response".to_string())])?;
        let mut result = BidderResponse::with_capacity(1);
        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = match bid.mtype.unwrap_or(0) {
                    1 => BidType::Banner,
                    2 => BidType::Video,
                    m => {
                        errs.push(BidderError::BadServerResponse(format!("Unsupported MType: {}", m)));
                        continue;
                    }
                };
                let bid_video = if bid_type == BidType::Video {
                    Some(get_bid_video(&bid))
                } else {
                    None
                };
                let bid_meta = get_bid_meta(&bid);
                let mut typed = TypedBid::new(bid, bid_type);
                typed.bid_video = bid_video;
                typed.bid_meta = bid_meta;
                result.bids.push(typed);
            }
        }
        if !errs.is_empty() && result.bids.is_empty() { return Err(errs); }
        Ok(result)
    }
}
