use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_bid_type_from_imp, get_bid_type_from_mtype};
use serde::Deserialize;

const CURRENCY_USD: &str = "USD";

pub struct ResetdigitalAdapter { pub endpoint: String }
impl ResetdigitalAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

#[derive(Debug, Deserialize, Default)]
struct ImpExtResetDigital {
    #[serde(rename = "placement_id", default)]
    placement_id: String,
}

impl Bidder for ResetdigitalAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.len() != 1 {
            return (vec![], vec![BidderError::BadInput(
                "ResetDigital adapter supports only one impression per request".to_string()
            )]);
        }

        let imp = &request.imp[0];

        // Parse bidder ext
        let bidder_val = match imp.ext.as_ref().and_then(|e| e.get("bidder")) {
            Some(v) => v.clone(),
            None => return (vec![], vec![BidderError::BadInput("Error parsing bidderExt from imp.ext: missing bidder".to_string())]),
        };
        let rd_ext: ImpExtResetDigital = match serde_json::from_value(bidder_val) {
            Ok(e) => e,
            Err(e) => return (vec![], vec![BidderError::BadInput(format!("Error parsing resetDigitalExt from bidderExt.bidder: {}", e))]),
        };

        // Build a copy of the request with only the fields resetdigital needs
        let mut imp_copy = imp.clone();
        if imp_copy.tagid.as_deref().unwrap_or("").is_empty() {
            imp_copy.tagid = Some(rd_ext.placement_id.clone());
        }

        // Validate/filter currencies - if none valid, default to USD
        let currencies = validate_currencies(request.cur.as_deref().unwrap_or(&[]));

        let mut req_copy = request.clone();
        req_copy.imp = vec![imp_copy];
        req_copy.cur = Some(currencies);

        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(format!("Error marshalling OpenRTB request: {}", e))]),
        };

        let uri = format!("{}?pid={}", self.endpoint, rd_ext.placement_id);

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
        headers.insert("X-OpenRTB-Version".to_string(), "2.6".to_string());

        (vec![RequestData {
            method: "POST".to_string(),
            uri,
            body,
            headers,
            imp_ids: vec![imp.id.clone()],
        }], vec![])
    }

    fn make_bids(&self, request: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code >= 400 && response.status_code < 500 {
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
            .map_err(|e| vec![BidderError::BadServerResponse(format!("Bad server response: {}", e))])?;

        if bid_resp.seatbid.is_empty() { return Ok(BidderResponse::new()); }

        let mut result = BidderResponse::with_capacity(bid_resp.seatbid.len());

        // Set currency from response or from request currencies
        if !bid_resp.cur.as_deref().unwrap_or("").is_empty() {
            result.currency = bid_resp.cur.clone().unwrap();
        } else {
            let currencies = validate_currencies(request.cur.as_deref().unwrap_or(&[]));
            result.currency = currencies.into_iter().next().unwrap_or_else(|| CURRENCY_USD.to_string());
        }

        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            let seat = if !sb.seat.as_deref().unwrap_or("").is_empty() {
                sb.seat.clone().unwrap()
            } else {
                "resetdigital".to_string()
            };
            for bid in sb.bid {
                // Filter out bids with price <= 0
                if bid.price <= 0.0 {
                    errs.push(BidderError::Unknown(format!("price {} <= 0 filtered out", bid.price)));
                    continue;
                }

                let bid_type = if bid.mtype.map(|m| m > 0).unwrap_or(false) {
                    get_bid_type_from_mtype(bid.mtype.unwrap())
                } else {
                    // fall back to imp-based detection
                    if let Some(imp) = request.imp.iter().find(|i| i.id == bid.impid) {
                        get_bid_type_from_imp(imp)
                    } else {
                        errs.push(BidderError::BadServerResponse(format!("no matching impression found for ImpID: {}", bid.impid)));
                        continue;
                    }
                };

                let typed_bid = TypedBid::new(bid, bid_type);
                // Store seat in orig_bid_cur field is not right; use a custom approach
                // The Go TypedBid has a Seat field; our Rust TypedBid doesn't, so we skip that
                // but we still include the bid
                let _ = seat.clone(); // seat info noted, but Rust TypedBid has no seat field
                result.bids.push(typed_bid);
            }
        }

        Ok(result)
    }
}

/// Validate and filter currency strings to ISO 4217 3-letter codes.
/// If no valid currencies, returns ["USD"].
fn validate_currencies(currencies: &[String]) -> Vec<String> {
    let valid: Vec<String> = currencies.iter()
        .map(|s| s.trim().to_uppercase())
        .filter(|s| !s.is_empty() && is_valid_iso4217(s))
        .collect();
    if valid.is_empty() {
        vec![CURRENCY_USD.to_string()]
    } else {
        valid
    }
}

/// Basic ISO 4217 validation: must be 3 uppercase ASCII letters.
fn is_valid_iso4217(s: &str) -> bool {
    s.len() == 3 && s.chars().all(|c| c.is_ascii_uppercase())
}
