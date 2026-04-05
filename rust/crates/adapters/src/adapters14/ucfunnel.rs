use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct UcfunnelAdapter { pub endpoint: String }
impl UcfunnelAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

fn get_bid_type_from_imp(imp: &openrtb::Imp) -> BidType {
    if imp.banner.is_some() { BidType::Banner }
    else if imp.video.is_some() { BidType::Video }
    else if imp.audio.is_some() { BidType::Audio }
    else if imp.native.is_some() { BidType::Native }
    else { BidType::Native }
}

impl Bidder for UcfunnelAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::BadInput("No impression in the bid request\n".to_string())]);
        }
        let bidder_ext = request.imp[0].ext.as_ref().and_then(|e| e.get("bidder"));
        let partner_id = bidder_ext
            .and_then(|b| b.get("partnerid"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let ad_unit_id = bidder_ext
            .and_then(|b| b.get("adunitid"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if partner_id.is_empty() || ad_unit_id.is_empty() {
            return (vec![], vec![BidderError::BadInput("No PartnerId or AdUnitId in the bid request\n".to_string())]);
        }
        let uri = format!("{}/{}/request", self.endpoint, urlencoding_simple(&partner_id));
        let body = match serde_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json".to_string());
        (vec![RequestData { method: "POST".to_string(), uri, body, headers, imp_ids: get_imp_ids(&request.imp) }], vec![])
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
        let mut result = BidderResponse::with_capacity(5);
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = internal.imp.iter().find(|i| i.id == bid.impid)
                    .map(get_bid_type_from_imp).unwrap_or(BidType::Native);
                // Only banner and video are valid per Go source
                if bid_type == BidType::Banner || bid_type == BidType::Video {
                    result.bids.push(TypedBid::new(bid, bid_type));
                }
            }
        }
        Ok(result)
    }
}

fn urlencoding_simple(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => out.push(c),
            _ => { for b in c.to_string().as_bytes() { out.push_str(&format!("%{:02X}", b)); } }
        }
    }
    out
}
