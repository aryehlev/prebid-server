use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;
use serde_json::Value;

pub struct BidstackAdapter {
    pub endpoint: String,
}

impl BidstackAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

#[derive(Deserialize)]
struct ExtImpBidder {
    bidder: Value,
}

#[derive(Deserialize)]
struct ImpExtBidstack {
    #[serde(rename = "publisherId", default)]
    publisher_id: String,
}

impl Bidder for BidstackAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _req_info: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::BadInput("missing impressions".to_string())]);
        }

        // Get publisher ID from first imp ext for Authorization header
        let ext_val = match &request.imp[0].ext {
            Some(v) => v.clone(),
            None => return (vec![], vec![BidderError::BadInput("get bidder ext: imp ext: missing ext".to_string())]),
        };

        let bidder_ext: ExtImpBidder = match serde_json::from_value(ext_val) {
            Ok(v) => v,
            Err(e) => return (vec![], vec![BidderError::BadInput(format!("get bidder ext: imp ext: {}", e))]),
        };

        let bidstack_ext: ImpExtBidstack = match serde_json::from_value(bidder_ext.bidder) {
            Ok(v) => v,
            Err(e) => return (vec![], vec![BidderError::BadInput(format!("get bidder ext: bidder ext: {}", e))]),
        };

        // Note: Go implementation converts imp.BidFloor currency to USD using reqInfo.ConvertCurrency.
        // Currency conversion is not yet supported in ExtraRequestInfo for Rust; bids are forwarded as-is.
        let mut req_copy = request.clone();
        for imp in &mut req_copy.imp {
            if imp.bidfloor.map(|f| f > 0.0).unwrap_or(false) {
                if let Some(cur) = &imp.bidfloorcur {
                    if cur.to_uppercase() == "USD" || cur.is_empty() {
                        // already USD, no conversion needed
                    }
                    // If non-USD, ideally we'd convert; skip for now as ExtraRequestInfo
                    // doesn't expose currency conversion in the Rust port.
                }
            }
        }

        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(format!("bid request marshal: {}", e))]),
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json".to_string());
        headers.insert("Authorization".to_string(), format!("Bearer {}", bidstack_ext.publisher_id));

        (vec![RequestData {
            method: "POST".to_string(),
            uri: self.endpoint.clone(),
            body,
            headers,
            imp_ids: get_imp_ids(&req_copy.imp),
        }], vec![])
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        match response.status_code {
            204 => return Ok(BidderResponse::new()),
            400 => return Err(vec![BidderError::BadInput("bad request from publisher".to_string())]),
            200 => {}
            _ => return Err(vec![BidderError::BadServerResponse(format!(
                "unexpected response status code: {}", response.status_code
            ))]),
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(format!("bid response unmarshal: {}", e))])?;

        let mut result = BidderResponse::with_capacity(internal.imp.len());
        if let Some(cur) = &bid_resp.cur {
            result.currency = cur.clone();
        }

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                result.bids.push(TypedBid::new(bid, BidType::Video));
            }
        }

        Ok(result)
    }
}
