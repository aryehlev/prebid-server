use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;

pub struct MinutemediaAdapter {
    pub endpoint: String,
}

impl MinutemediaAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

const TEST_ENDPOINT: &str = "https://pbs.minutemedia-prebid.com/pbs-test";

/// MinuteMedia bidder extension from imp.ext.bidder
#[derive(Debug, Default, Deserialize)]
struct ImpExtMinuteMedia {
    #[serde(rename = "org", default)]
    org: String,
}

fn extract_org(request: &openrtb::BidRequest) -> Result<String, BidderError> {
    if request.imp.is_empty() {
        return Err(BidderError::BadInput("no imps in bid request".to_string()));
    }

    let bidder_ext = request.imp[0].ext.as_ref()
        .and_then(|e| e.get("bidder"))
        .ok_or_else(|| BidderError::BadInput("failed to unmarshal bidderExt".to_string()))?;

    let imp_ext: ImpExtMinuteMedia = serde_json::from_value(bidder_ext.clone())
        .map_err(|e| BidderError::BadInput(format!("failed to unmarshal ImpExtMinuteMedia: {}", e)))?;

    let org = imp_ext.org.trim().to_string();
    Ok(org)
}

fn get_media_type_for_bid(bid: &openrtb::Bid) -> Result<BidType, BidderError> {
    // mtype is in bid.ext since openrtb struct doesn't have it
    let mtype = bid.ext
        .as_ref()
        .and_then(|e| e.get("mtype"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;

    match mtype {
        1 => Ok(BidType::Banner),
        2 => Ok(BidType::Video),
        _ => Err(BidderError::BadServerResponse(format!("unsupported MType {}", mtype))),
    }
}

fn percent_encode(s: &str) -> String {
    let mut result = String::new();
    for c in s.chars() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => result.push(c),
            _ => {
                for byte in c.to_string().as_bytes() {
                    result.push_str(&format!("%{:02X}", byte));
                }
            }
        }
    }
    result
}

impl Bidder for MinutemediaAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let org = match extract_org(request) {
            Ok(o) => o,
            Err(e) => return (vec![], vec![e]),
        };

        let body = match serde_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());

        let base_endpoint = if request.test == Some(1) {
            TEST_ENDPOINT
        } else {
            &self.endpoint
        };

        let uri = format!("{}?publisher_id={}", base_endpoint, percent_encode(&org));

        (
            vec![RequestData {
                method: "POST".to_string(),
                uri,
                body,
                headers,
                imp_ids: get_imp_ids(&request.imp),
            }],
            vec![],
        )
    }

    fn make_bids(
        &self,
        internal: &openrtb::BidRequest,
        _external: &RequestData,
        response: &ResponseData,
    ) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 {
            return Ok(BidderResponse::new());
        }
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

        let bid_response: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(internal.imp.len());
        if let Some(cur) = &bid_response.cur {
            if !cur.is_empty() {
                result.currency = cur.clone();
            }
        }

        let mut errs = Vec::new();

        for seat_bid in bid_response.seatbid {
            for bid in seat_bid.bid {
                match get_media_type_for_bid(&bid) {
                    Ok(bid_type) => {
                        result.bids.push(TypedBid::new(bid, bid_type));
                    }
                    Err(e) => {
                        errs.push(e);
                    }
                }
            }
        }

        if !errs.is_empty() {
            // Return partial results with errors
        }

        Ok(result)
    }
}
