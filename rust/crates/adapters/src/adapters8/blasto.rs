use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_bid_type_from_imp, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;
use serde_json::Value;

pub struct BlastoAdapter {
    pub endpoint: String,
}

impl BlastoAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

#[derive(Deserialize)]
struct ExtImpBidder {
    bidder: Value,
}

#[derive(Deserialize, Default)]
struct ExtBlasto {
    #[serde(rename = "accountId", default)]
    account_id: String,
    #[serde(rename = "sourceId", default)]
    source_id: String,
}

impl Bidder for BlastoAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::BadInput("ext.bidder not provided".to_string())]);
        }

        let ext_val = match &request.imp[0].ext {
            Some(v) => v.clone(),
            None => return (vec![], vec![BidderError::BadInput("ext.bidder not provided".to_string())]),
        };

        let bidder_ext: ExtImpBidder = match serde_json::from_value(ext_val) {
            Ok(v) => v,
            Err(_) => return (vec![], vec![BidderError::BadInput("ext.bidder not provided".to_string())]),
        };

        let blasto_ext: ExtBlasto = match serde_json::from_value(bidder_ext.bidder) {
            Ok(v) => v,
            Err(_) => return (vec![], vec![BidderError::BadInput("ext.bidder not provided".to_string())]),
        };

        // Build URL: replace {{.AccountID}} and {{.SourceId}}
        let url = self.endpoint
            .replace("{{.AccountID}}", &blasto_ext.account_id)
            .replace("{{.SourceId}}", &blasto_ext.source_id);

        // Clear imp.ext
        let mut req_copy = request.clone();
        for imp in &mut req_copy.imp {
            imp.ext = None;
        }

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
        headers.insert("X-Openrtb-Version".to_string(), "2.5".to_string());

        if let Some(device) = &request.device {
            if let Some(ua) = &device.ua {
                if !ua.is_empty() {
                    headers.insert("User-Agent".to_string(), ua.clone());
                }
            }
            if let Some(ipv6) = &device.ipv6 {
                if !ipv6.is_empty() {
                    headers.insert("X-Forwarded-For".to_string(), ipv6.clone());
                }
            }
            if let Some(ip) = &device.ip {
                if !ip.is_empty() {
                    headers.insert("X-Forwarded-For".to_string(), ip.clone());
                }
            }
        }

        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        (vec![RequestData {
            method: "POST".to_string(),
            uri: url,
            body,
            headers,
            imp_ids: get_imp_ids(&request.imp),
        }], vec![])
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code == 400 {
            return Err(vec![BidderError::BadInput(format!(
                "Unexpected status code: [ {} ]", response.status_code
            ))]);
        }
        if response.status_code == 503 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Something went wrong, please contact your Account Manager. Status Code: [ {} ] ", response.status_code
            ))]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected status code: [ {} ]. Run with request.debug = 1 for more info", response.status_code
            ))]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|_| vec![BidderError::BadServerResponse("Bad Server Response".to_string())])?;

        if bid_resp.seatbid.is_empty() {
            return Err(vec![BidderError::BadServerResponse("Empty SeatBid array".to_string())]);
        }

        let sb = &bid_resp.seatbid[0];
        let mut result = BidderResponse::with_capacity(sb.bid.len());

        for bid in &sb.bid {
            let bid_type = internal.imp.iter()
                .find(|i| i.id == bid.impid)
                .map(|imp| {
                    if imp.video.is_some() { BidType::Video }
                    else if imp.native.is_some() { BidType::Native }
                    else { BidType::Banner }
                })
                .unwrap_or(BidType::Banner);
            result.bids.push(TypedBid::new(bid.clone(), bid_type));
        }

        Ok(result)
    }
}
