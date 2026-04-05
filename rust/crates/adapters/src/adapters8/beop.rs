use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;
use serde_json::Value;

pub struct BeopAdapter {
    pub endpoint: String,
}

impl BeopAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

#[derive(Deserialize)]
struct ExtImpBidder {
    bidder: Value,
}

#[derive(Deserialize, Default)]
struct ExtImpBeop {
    #[serde(rename = "pid", default)]
    beop_publisher_id: String,
    #[serde(rename = "nid", default)]
    beop_network_id: String,
    #[serde(rename = "nptnid", default)]
    beop_network_partner_id: String,
}

fn get_media_type_for_bid(bid: &openrtb::Bid) -> Result<BidType, BidderError> {
    match bid.mtype {
        Some(1) => Ok(BidType::Banner),
        Some(2) => Ok(BidType::Video),
        _ => Err(BidderError::BadServerResponse(format!(
            "Failed to parse bid mType for impression \"{}\"", bid.impid
        ))),
    }
}

impl Bidder for BeopAdapter {
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

        let beop_ext: ExtImpBeop = match serde_json::from_value(bidder_ext.bidder) {
            Ok(v) => v,
            Err(_) => return (vec![], vec![BidderError::BadInput("ext.bidder not provided".to_string())]),
        };

        // Build URL with query params
        let mut query_parts = Vec::new();
        if !beop_ext.beop_publisher_id.is_empty() {
            query_parts.push(format!("pid={}", beop_ext.beop_publisher_id));
        }
        if !beop_ext.beop_network_id.is_empty() {
            query_parts.push(format!("nid={}", beop_ext.beop_network_id));
        }
        if !beop_ext.beop_network_partner_id.is_empty() {
            query_parts.push(format!("nptnid={}", beop_ext.beop_network_partner_id));
        }

        let url = if query_parts.is_empty() {
            self.endpoint.clone()
        } else {
            format!("{}?{}", self.endpoint, query_parts.join("&"))
        };

        let body = match serde_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        (vec![RequestData {
            method: "POST".to_string(),
            uri: url,
            body,
            headers,
            imp_ids: get_imp_ids(&request.imp),
        }], vec![])
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code == 503 {
            return Err(vec![BidderError::BadInput(format!(
                "Service Unavailable. Status Code: [ {} ] ", response.status_code
            ))]);
        }
        if response.status_code == 400 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info.", response.status_code
            ))]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|_| vec![BidderError::BadServerResponse("Bad Server Response".to_string())])?;

        if bid_resp.seatbid.is_empty() {
            return Err(vec![BidderError::BadServerResponse("Empty SeatBid array".to_string())]);
        }

        let sb = &bid_resp.seatbid[0];
        let mut result = BidderResponse::with_capacity(sb.bid.len());
        let mut errs = Vec::new();

        for bid in &sb.bid {
            match get_media_type_for_bid(bid) {
                Ok(bt) => result.bids.push(TypedBid::new(bid.clone(), bt)),
                Err(e) => errs.push(e),
            }
        }

        if result.bids.is_empty() {
            return Err(errs);
        }

        Ok(result)
    }
}
