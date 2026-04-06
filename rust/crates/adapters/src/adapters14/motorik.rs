use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use openrtb_ext::BidType;
use serde::Deserialize;
use serde_json::Value;

pub struct MotorikAdapter { pub endpoint: String }
impl MotorikAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

#[derive(Deserialize)]
struct ExtImpBidder { bidder: Value }

#[derive(Deserialize)]
struct ExtMotorik {
    #[serde(rename = "accountId", default)]
    account_id: String,
    #[serde(rename = "placementId", default)]
    placement_id: String,
}

fn get_headers(request: &openrtb::BidRequest) -> HashMap<String, String> {
    let mut h = HashMap::new();
    h.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
    h.insert("Accept".to_string(), "application/json".to_string());
    h.insert("X-Openrtb-Version".to_string(), "2.5".to_string());
    if let Some(device) = &request.device {
        if let Some(ua) = &device.ua { if !ua.is_empty() { h.insert("User-Agent".to_string(), ua.clone()); } }
        if let Some(ip6) = &device.ipv6 { if !ip6.is_empty() { h.insert("X-Forwarded-For".to_string(), ip6.clone()); } }
        if let Some(ip) = &device.ip { if !ip.is_empty() { h.insert("X-Forwarded-For".to_string(), ip.clone()); } }
    }
    h
}

fn get_media_type_for_imp(imp_id: &str, imps: &[openrtb::Imp]) -> Result<BidType, BidderError> {
    for imp in imps {
        if imp.id == imp_id {
            if imp.banner.is_some() { return Ok(BidType::Banner); }
            if imp.video.is_some() { return Ok(BidType::Video); }
            if imp.native.is_some() { return Ok(BidType::Native); }
            return Err(BidderError::BadInput(format!("Failed to find impression \"{}\"", imp_id)));
        }
    }
    Err(BidderError::BadInput(format!("Failed to find impression \"{}\"", imp_id)))
}

impl Bidder for MotorikAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::BadInput("no impressions".to_string())]);
        }
        let imp = &request.imp[0];
        let bidder_ext: ExtImpBidder = match imp.ext.as_ref()
            .and_then(|e| serde_json::from_value(e.clone()).ok()) {
            Some(v) => v,
            None => return (vec![], vec![BidderError::BadInput("Error parsing motorikExt - failed to parse".to_string())]),
        };
        let motorik_ext: ExtMotorik = match serde_json::from_value(bidder_ext.bidder) {
            Ok(v) => v,
            Err(e) => return (vec![], vec![BidderError::BadInput(format!("Error parsing bidderExt - {}", e))]),
        };

        let uri = self.endpoint
            .replace("{{.AccountID}}", &motorik_ext.account_id)
            .replace("{{.SourceId}}", &motorik_ext.placement_id);

        let mut req_copy = request.clone();
        req_copy.imp[0].ext = None;
        let headers = get_headers(request);
        let imp_ids: Vec<String> = req_copy.imp.iter().map(|i| i.id.clone()).collect();
        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };
        (vec![RequestData { method: "POST".to_string(), uri, body, headers, imp_ids }], vec![])
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code == 503 {
            return Err(vec![BidderError::BadServerResponse(format!("Something went wrong Status Code: [ {} ] ", response.status_code))]);
        }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|_| vec![BidderError::BadServerResponse("Bad Server Response".to_string())])?;
        if bid_resp.seatbid.is_empty() {
            return Err(vec![BidderError::BadServerResponse("Empty SeatBid array".to_string())]);
        }
        let mut result = BidderResponse::with_capacity(5);
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = get_media_type_for_imp(&bid.impid, &internal.imp)
                    .map_err(|e| vec![e])?;
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        Ok(result)
    }
}
