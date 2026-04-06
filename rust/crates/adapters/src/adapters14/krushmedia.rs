use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct KrushmediaAdapter { pub endpoint: String }
impl KrushmediaAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

fn get_bid_type_from_imp(imp: &openrtb::Imp) -> BidType {
    if imp.video.is_some() { return BidType::Video; }
    if imp.native.is_some() { return BidType::Native; }
    BidType::Banner
}

impl Bidder for KrushmediaAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::BadInput("Missing Imp Object".to_string())]);
        }

        // Extract accountId from first imp.ext.bidder
        let bidder = request.imp[0].ext.as_ref()
            .and_then(|e| e.get("bidder"))
            .cloned()
            .unwrap_or(serde_json::Value::Null);

        if bidder.is_null() {
            return (vec![], vec![BidderError::BadInput("Bidder extension not provided or can't be unmarshalled".to_string())]);
        }

        let account_id = bidder.get("accountId").and_then(|v| v.as_str()).unwrap_or("").to_string();

        // Build URL from template: replace {{.AccountID}}
        let url = self.endpoint.replace("{{.AccountID}}", &account_id);

        // Clear first imp ext
        let mut req = request.clone();
        req.imp[0].ext = None;

        let body = match serde_json::to_vec(&req) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
        headers.insert("X-Openrtb-Version".to_string(), "2.5".to_string());
        if let Some(device) = &request.device {
            if let Some(ua) = &device.ua { if !ua.is_empty() { headers.insert("User-Agent".to_string(), ua.clone()); } }
            if let Some(ip) = &device.ip { if !ip.is_empty() { headers.insert("X-Forwarded-For".to_string(), ip.clone()); } }
            if let Some(lang) = &device.language { if !lang.is_empty() { headers.insert("Accept-Language".to_string(), lang.clone()); } }
            if let Some(dnt) = device.dnt { headers.insert("Dnt".to_string(), dnt.to_string()); }
        }

        (vec![RequestData {
            method: "POST".to_string(),
            uri: url,
            body,
            headers,
            imp_ids: get_imp_ids(&request.imp),
        }], vec![])
    }

    fn make_bids(&self, request: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code == 400 {
            return Err(vec![BidderError::BadInput(
                format!("Unexpected status code: [ {} ]", response.status_code)
            )]);
        }
        if response.status_code == 503 {
            return Ok(BidderResponse::new());
        }
        if let Err(_e) = crate::check_response_status(response.status_code) {
            return Err(vec![BidderError::BadServerResponse(
                format!("Something went wrong, please contact your Account Manager. Status Code: [ {} ] ", response.status_code)
            )]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|_| vec![BidderError::BadServerResponse("Bad Server Response".to_string())])?;

        if bid_resp.seatbid.is_empty() {
            return Ok(BidderResponse::new());
        }

        let sb = &bid_resp.seatbid[0];
        let mut result = BidderResponse::with_capacity(sb.bid.len());
        for bid in &sb.bid {
            let bid_type = request.imp.iter()
                .find(|i| i.id == bid.impid)
                .map(get_bid_type_from_imp)
                .unwrap_or(BidType::Banner);
            result.bids.push(TypedBid::new(bid.clone(), bid_type));
        }
        Ok(result)
    }
}
