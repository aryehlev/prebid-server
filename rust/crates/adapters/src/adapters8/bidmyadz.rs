use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;

pub struct BidmyadzAdapter {
    pub endpoint: String,
}

impl BidmyadzAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

#[derive(Deserialize, Default)]
struct BidmyadzBidExt {
    #[serde(rename = "mediaType", default)]
    media_type: String,
}

fn parse_bid_type(s: &str) -> Result<BidType, BidderError> {
    match s {
        "banner" => Ok(BidType::Banner),
        "video" => Ok(BidType::Video),
        "native" => Ok(BidType::Native),
        "audio" => Ok(BidType::Audio),
        other => Err(BidderError::BadServerResponse(format!(
            "BidExt parsing error. invalid media type: {}", other
        ))),
    }
}

impl Bidder for BidmyadzAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errors = Vec::new();

        if request.imp.len() > 1 {
            errors.push(BidderError::BadInput("Bidder does not support multi impression".to_string()));
        }

        let device = request.device.as_ref();
        let ip_empty = device.and_then(|d| d.ip.as_deref()).unwrap_or("").is_empty();
        let ipv6_empty = device.and_then(|d| d.ipv6.as_deref()).unwrap_or("").is_empty();
        let ua_empty = device.and_then(|d| d.ua.as_deref()).unwrap_or("").is_empty();

        if ip_empty && ipv6_empty {
            errors.push(BidderError::BadInput("IP/IPv6 is a required field".to_string()));
        }
        if ua_empty {
            errors.push(BidderError::BadInput("User-Agent is a required field".to_string()));
        }

        if !errors.is_empty() {
            return (vec![], errors);
        }

        let body = match serde_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
        headers.insert("X-Openrtb-Version".to_string(), "2.5".to_string());

        (vec![RequestData {
            method: "POST".to_string(),
            uri: self.endpoint.clone(),
            body,
            headers,
            imp_ids: get_imp_ids(&request.imp),
        }], vec![])
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code == 400 {
            return Err(vec![BidderError::BadInput(format!(
                "Bad Request. {}", String::from_utf8_lossy(&response.body)
            ))]);
        }
        if response.status_code == 503 {
            return Err(vec![BidderError::BadServerResponse(
                "Bidder is unavailable. Please contact your account manager.".to_string()
            )]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Something went wrong. Status Code: [ {} ] {}", response.status_code,
                String::from_utf8_lossy(&response.body)
            ))]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        if bid_resp.seatbid.is_empty() {
            return Err(vec![BidderError::BadServerResponse("Empty SeatBid".to_string())]);
        }

        let bids = &bid_resp.seatbid[0].bid;
        if bids.is_empty() {
            return Err(vec![BidderError::BadServerResponse("Empty SeatBid.Bids".to_string())]);
        }

        let bid = bids[0].clone();

        let ext_val = bid.ext.as_ref()
            .ok_or_else(|| vec![BidderError::BadServerResponse("BidExt parsing error. missing ext".to_string())])?;

        let bid_ext: BidmyadzBidExt = serde_json::from_value(ext_val.clone())
            .map_err(|e| vec![BidderError::BadServerResponse(format!("BidExt parsing error. {}", e))])?;

        let bid_type = parse_bid_type(&bid_ext.media_type)
            .map_err(|e| vec![e])?;

        let mut result = BidderResponse::with_capacity(1);
        result.bids.push(TypedBid::new(bid, bid_type));
        Ok(result)
    }
}
