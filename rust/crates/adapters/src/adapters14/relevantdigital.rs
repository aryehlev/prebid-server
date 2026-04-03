use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_bid_type_from_mtype, get_imp_ids};
use openrtb_ext::BidType;

pub struct RelevantdigitalAdapter { pub endpoint: String }
impl RelevantdigitalAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

fn get_bid_type_from_ext(bid: &openrtb::Bid) -> Result<BidType, BidderError> {
    let t = bid.ext.as_ref()
        .and_then(|e| e.get("prebid"))
        .and_then(|p| p.get("type"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    match t {
        "banner" => Ok(BidType::Banner),
        "video" => Ok(BidType::Video),
        "native" => Ok(BidType::Native),
        "audio" => Ok(BidType::Audio),
        _ => Err(BidderError::BadServerResponse(format!("failed to parse bid type, missing ext: {}", bid.impid))),
    }
}

impl Bidder for RelevantdigitalAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let body = match serde_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
        (vec![RequestData { method: "POST".to_string(), uri: self.endpoint.clone(), body, headers, imp_ids: get_imp_ids(&request.imp) }], vec![])
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(5);
        if let Some(cur) = &bid_resp.cur { if !cur.is_empty() { result.currency = cur.clone(); } }
        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                // Try mtype first, then fall back to ext.prebid.type
                let bid_type = if let Some(mtype) = bid.mtype {
                    if mtype > 0 {
                        get_bid_type_from_mtype(mtype)
                    } else {
                        match get_bid_type_from_ext(&bid) {
                            Ok(t) => t,
                            Err(e) => { errs.push(e); continue; }
                        }
                    }
                } else {
                    match get_bid_type_from_ext(&bid) {
                        Ok(t) => t,
                        Err(e) => { errs.push(e); continue; }
                    }
                };
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        if !errs.is_empty() && result.bids.is_empty() { return Err(errs); }
        Ok(result)
    }
}
