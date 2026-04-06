use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids, check_response_status};
use openrtb::BidResponse;
use openrtb_ext::BidType;

pub struct OrbidderAdapter { pub endpoint: String }
impl OrbidderAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

fn get_bid_type_from_mtype(mtype: u64, imp_id: &str) -> Result<BidType, BidderError> {
    match mtype {
        1 => Ok(BidType::Banner),
        2 => Ok(BidType::Video),
        3 => Ok(BidType::Audio),
        4 => Ok(BidType::Native),
        _ => Err(BidderError::BadInput(
            format!("Could not define media type for impression: {}", imp_id)
        )),
    }
}

impl Bidder for OrbidderAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut valid_imps: Vec<openrtb::Imp> = Vec::new();

        for imp in &request.imp {
            // Validate ext.bidder is present and parseable
            let has_bidder_ext = imp.ext.as_ref()
                .and_then(|e| e.get("bidder"))
                .is_some();
            if !has_bidder_ext {
                errs.push(BidderError::BadInput("missing bidder ext".to_string()));
                continue;
            }
            // Set bidfloor currency to EUR (no actual conversion since ExtraRequestInfo lacks it)
            let mut imp_copy = imp.clone();
            if let Some(ref cur) = imp.bidfloorcur {
                if cur.to_uppercase() != "EUR" {
                    imp_copy.bidfloorcur = Some("EUR".to_string());
                    // Note: actual currency conversion would require ExtraRequestInfo.convert_currency
                    // which is not available in this Rust port; we just reset the currency marker
                }
            } else {
                imp_copy.bidfloorcur = Some("EUR".to_string());
            }
            valid_imps.push(imp_copy);
        }

        if valid_imps.is_empty() {
            return (vec![], errs);
        }

        let mut req_copy = request.clone();
        req_copy.imp = valid_imps;

        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => {
                errs.push(BidderError::BadInput(e.to_string()));
                return (vec![], errs);
            }
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        let imp_ids = get_imp_ids(&req_copy.imp);
        (vec![RequestData {
            method: "POST".to_string(),
            uri: self.endpoint.clone(),
            body,
            headers,
            imp_ids,
        }], errs)
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code >= 500 {
            return Err(vec![BidderError::BadServerResponse(
                format!("Unexpected status code: {}. Dsp server internal error.", response.status_code)
            )]);
        }
        if response.status_code >= 400 {
            return Err(vec![BidderError::BadInput(
                format!("Unexpected status code: {}. Bad request to dsp.", response.status_code)
            )]);
        }
        if let Err(e) = check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(5);
        if let Some(cur) = &bid_resp.cur {
            if !cur.is_empty() { result.currency = cur.clone(); }
        }
        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let mtype = bid.ext.as_ref()
                    .and_then(|e| e.get("mtype"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                match get_bid_type_from_mtype(mtype, &bid.impid) {
                    Ok(t) => result.bids.push(TypedBid::new(bid, t)),
                    Err(e) => errs.push(e),
                }
            }
        }
        if !errs.is_empty() { return Err(errs); }
        Ok(result)
    }
}
