use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids, check_response_status};
use openrtb::BidResponse;
use openrtb_ext::BidType;

pub struct OmsAdapter { pub endpoint: String }
impl OmsAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

fn get_bid_type_from_mtype(mtype: u64) -> BidType {
    match mtype {
        2 => BidType::Video,
        _ => BidType::Banner,
    }
}

impl Bidder for OmsAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        // Build URI with publisherId query param from first imp ext.bidder
        let mut uri = self.endpoint.clone();
        if let Some(imp) = request.imp.first() {
            if imp.ext.is_some() {
                let pid = imp.ext.as_ref()
                    .and_then(|e| e.get("bidder"))
                    .and_then(|b| b.get("pid"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let publisher_id_num = imp.ext.as_ref()
                    .and_then(|e| e.get("bidder"))
                    .and_then(|b| b.get("publisherId"))
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);

                if !pid.is_empty() {
                    uri = format!("{}?publisherId={}", self.endpoint, pid);
                } else if publisher_id_num > 0 {
                    uri = format!("{}?publisherId={}", self.endpoint, publisher_id_num);
                }
            }
        }

        let body = match serde_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
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
        // Go: if len(response.Cur) == 0, set currency — that's a bug in Go, we skip it
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let mtype = bid.ext.as_ref()
                    .and_then(|e| e.get("mtype"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let bid_type = get_bid_type_from_mtype(mtype);
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        Ok(result)
    }
}
