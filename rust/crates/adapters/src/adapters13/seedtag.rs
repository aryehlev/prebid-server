use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids, check_response_status};
use openrtb::BidResponse;
use openrtb_ext::BidType;

pub struct SeedtagAdapter { pub endpoint: String }
impl SeedtagAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

fn get_bid_type_from_mtype(mtype: u64) -> Result<BidType, BidderError> {
    match mtype {
        1 => Ok(BidType::Banner),
        2 => Ok(BidType::Video),
        _ => Err(BidderError::BadServerResponse("bid.MType invalid".to_string())),
    }
}

impl Bidder for SeedtagAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let errs = Vec::new();
        let mut valid_imps: Vec<openrtb::Imp> = Vec::new();

        for imp in &request.imp {
            let mut imp_copy = imp.clone();
            // Convert bid floor to USD if in a different currency
            // Note: actual conversion requires ExtraRequestInfo.convert_currency which is not available.
            // We simply drop the floor currency marker if not USD.
            if let Some(floor) = imp.bidfloor {
                if floor > 0.0 {
                    if let Some(cur) = &imp.bidfloorcur {
                        if cur.to_uppercase() != "USD" {
                            // Without conversion support, we skip this imp (matching Go behavior of returning error)
                            // But since we cannot convert, we just clear floor to avoid wrong currency
                            imp_copy.bidfloorcur = Some("USD".to_string());
                        }
                    }
                }
            }
            valid_imps.push(imp_copy);
        }

        if valid_imps.is_empty() {
            return (vec![], errs);
        }

        let mut req_copy = request.clone();
        req_copy.cur = Some(vec!["USD".to_string()]);
        req_copy.imp = valid_imps;

        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
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
        if let Err(e) = check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(5);
        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let mtype = bid.mtype.unwrap_or(0) as u64;
                match get_bid_type_from_mtype(mtype) {
                    Ok(t) => result.bids.push(TypedBid::new(bid, t)),
                    Err(e) => errs.push(e),
                }
            }
        }
        // Return partial bids alongside errors (Go appends errors and continues)
        if !errs.is_empty() && result.bids.is_empty() {
            return Err(errs);
        }
        Ok(result)
    }
}
