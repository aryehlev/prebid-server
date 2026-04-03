use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_bid_type_from_imp, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;
use serde_json::Value;

pub struct AcuityadsAdapter { pub endpoint: String }
impl AcuityadsAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

#[derive(Deserialize)]
struct ExtImpBidder { bidder: Value }

#[derive(Deserialize)]
struct ExtAcuityAds {
    #[serde(default)]
    host: String,
    #[serde(rename = "accountid", default)]
    account_id: String,
}

impl Bidder for AcuityadsAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::BadInput("no impressions".to_string())]);
        }
        let imp = &request.imp[0];
        let bidder_ext: ExtImpBidder = match imp.ext.as_ref()
            .and_then(|e| serde_json::from_value(e.clone()).ok()) {
            Some(v) => v,
            None => return (vec![], vec![BidderError::BadInput("ext.bidder not provided".to_string())]),
        };
        let ext: ExtAcuityAds = match serde_json::from_value(bidder_ext.bidder) {
            Ok(v) => v,
            Err(_) => return (vec![], vec![BidderError::BadInput("ext.bidder not provided".to_string())]),
        };
        let uri = self.endpoint
            .replace("{{.Host}}", &ext.host)
            .replace("{{.AccountID}}", &ext.account_id);

        let body = match serde_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
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
        (vec![RequestData { method: "POST".to_string(), uri, body, headers, imp_ids: get_imp_ids(&request.imp) }], vec![])
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|_| vec![BidderError::BadServerResponse("Bad Server Response".to_string())])?;
        if bid_resp.seatbid.is_empty() {
            return Err(vec![BidderError::BadServerResponse("Empty SeatBid array".to_string())]);
        }
        let mut result = BidderResponse::with_capacity(5);
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = internal.imp.iter().find(|i| i.id == bid.impid).map(get_bid_type_from_imp).unwrap_or(BidType::Banner);
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        Ok(result)
    }
}
