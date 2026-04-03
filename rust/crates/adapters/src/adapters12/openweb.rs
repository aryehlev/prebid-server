use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids, check_response_status};
use openrtb::BidResponse;
use openrtb_ext::BidType;

pub struct OpenwebAdapter { pub endpoint: String }
impl OpenwebAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

fn get_bid_type_from_mtype(mtype: u64, _imp_id: &str) -> Result<BidType, BidderError> {
    match mtype {
        1 => Ok(BidType::Banner),
        2 => Ok(BidType::Video),
        _ => Err(BidderError::BadServerResponse(format!("unsupported MType {}", mtype))),
    }
}

/// Extract org (publisher_id) from imp.ext.bidder: check org field, then aid field.
/// Also validates placementId is present.
fn extract_org(request: &openrtb::BidRequest) -> Result<String, BidderError> {
    for imp in &request.imp {
        let bidder = imp.ext.as_ref()
            .and_then(|e| e.get("bidder"))
            .ok_or_else(|| BidderError::BadInput("unmarshal bidderExt: missing bidder".to_string()))?;

        let placement_id = bidder.get("placementId")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if placement_id.is_empty() {
            return Err(BidderError::BadInput("no placement id supplied".to_string()));
        }

        let org = bidder.get("org")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if !org.is_empty() {
            return Ok(org);
        }

        let aid = bidder.get("aid")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        if aid != 0 {
            return Ok(aid.to_string());
        }
    }
    Err(BidderError::BadInput("no org or aid supplied".to_string()))
}

impl Bidder for OpenwebAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let org = match extract_org(request) {
            Ok(o) => o,
            Err(e) => return (vec![], vec![e]),
        };

        let body = match serde_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let uri = format!("{}?publisher_id={}", self.endpoint, org);
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        (vec![RequestData {
            method: "POST".to_string(),
            uri,
            body,
            headers,
            imp_ids: get_imp_ids(&request.imp),
        }], vec![])
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(5);
        if let Some(cur) = &bid_resp.cur {
            if !cur.is_empty() { result.currency = cur.clone(); }
        }
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let mtype = bid.mtype.unwrap_or(0) as u64;
                match get_bid_type_from_mtype(mtype, &bid.impid) {
                    Ok(t) => result.bids.push(TypedBid::new(bid, t)),
                    Err(_) => {},
                }
            }
        }
        Ok(result)
    }
}
