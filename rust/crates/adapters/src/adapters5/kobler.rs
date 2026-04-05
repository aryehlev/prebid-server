use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;

const DEV_ENDPOINT: &str = "https://bid-service.dev.essrtb.com/bid/prebid_server_rtb_call";
const SUPPORTED_CURRENCY: &str = "USD";

pub struct KoblerAdapter { pub endpoint: String }
impl KoblerAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

#[derive(Deserialize, Default)]
struct KoblerImpExt {
    #[serde(rename = "test", default)]
    test: bool,
}

/// Read bid type from bid.ext.prebid.type, fall back to Banner.
fn get_bid_type_for_bid(bid: &openrtb::Bid) -> BidType {
    if let Some(ext) = &bid.ext {
        if let Some(prebid_val) = ext.get("prebid") {
            if let Ok(prebid) = serde_json::from_value::<openrtb_ext::ExtBidPrebid>(prebid_val.clone()) {
                if let Some(bt) = prebid.bid_type {
                    return bt;
                }
            }
        }
    }
    BidType::Banner
}

impl Bidder for KoblerAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut req_copy = request.clone();
        let mut errs = Vec::new();

        // Sanitize: remove user, clear device IP
        req_copy.user = None;
        if let Some(device) = req_copy.device.as_mut() {
            device.ip = None;
            device.ipv6 = None;
        }

        // Ensure USD is in cur list
        let has_usd = req_copy.cur.as_deref().unwrap_or(&[]).iter().any(|c| c == SUPPORTED_CURRENCY);
        if !has_usd {
            req_copy.cur.get_or_insert_with(Vec::new).push(SUPPORTED_CURRENCY.to_string());
        }

        // Check first imp for test mode (match Go: continue on parse errors, don't return).
        let mut test_mode = false;
        'test_mode: {
            if let Some(first_imp) = req_copy.imp.first() {
                if let Some(ext) = &first_imp.ext {
                    let bidder_val = match ext.get("bidder") {
                        Some(v) => v.clone(),
                        None => break 'test_mode,
                    };
                    // Parse outer bidder ext
                    let bidder_ext: serde_json::Value = match serde_json::from_value(bidder_val) {
                        Ok(v) => v,
                        Err(_e) => {
                            errs.push(BidderError::BadInput("Error parsing bidderExt object".to_string()));
                            break 'test_mode;  // continue to use default endpoint
                        }
                    };
                    match serde_json::from_value::<KoblerImpExt>(bidder_ext) {
                        Ok(imp_ext) => test_mode = imp_ext.test,
                        Err(_e) => {
                            errs.push(BidderError::BadInput("Error parsing impExt object".to_string()));
                            // continue with test_mode = false
                        }
                    }
                }
            }
        }

        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let endpoint = if test_mode { DEV_ENDPOINT } else { &self.endpoint };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());

        (
            vec![RequestData {
                method: "POST".to_string(),
                uri: endpoint.to_string(),
                body,
                headers,
                imp_ids: get_imp_ids(&req_copy.imp),
            }],
            errs,
        )
    }

    fn make_bids(&self, _internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 {
            return Ok(BidderResponse::new());
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info.",
                response.status_code
            ))]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(5);
        if let Some(cur) = bid_resp.cur {
            result.currency = cur;
        }

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = get_bid_type_for_bid(&bid);
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }

        Ok(result)
    }
}
