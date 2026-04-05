use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

const ADAPTER_VERSION: &str = "1.0.0";

pub struct ConcertAdapter { pub endpoint: String }
impl ConcertAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

fn get_bid_type_from_mtype(mtype: i32) -> Result<BidType, BidderError> {
    match mtype {
        1 => Ok(BidType::Banner),
        2 => Ok(BidType::Video),
        3 => Ok(BidType::Audio),
        _ => Err(BidderError::BadServerResponse(
            format!("Failed to parse media type for bid with mtype {}", mtype)
        )),
    }
}

impl Bidder for ConcertAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![]);
        }

        // Extract partnerId from first imp.ext.bidder
        let partner_id = request.imp[0].ext.as_ref()
            .and_then(|e| e.get("bidder"))
            .and_then(|b| b.get("partnerId"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // Build request JSON then inject ext.adapterVersion and ext.partnerId
        let mut req_map: serde_json::Map<String, serde_json::Value> = match serde_json::to_value(request) {
            Ok(serde_json::Value::Object(m)) => m,
            _ => return (vec![], vec![BidderError::BadInput("failed to serialize request".to_string())]),
        };

        let ext = req_map.entry("ext".to_string())
            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
        if let serde_json::Value::Object(ext_map) = ext {
            ext_map.insert("adapterVersion".to_string(), serde_json::Value::String(ADAPTER_VERSION.to_string()));
            ext_map.insert("partnerId".to_string(), serde_json::Value::String(partner_id));
        }

        let body = match serde_json::to_vec(&req_map) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        (vec![RequestData {
            method: "POST".to_string(),
            uri: self.endpoint.clone(),
            body,
            headers,
            imp_ids: get_imp_ids(&request.imp),
        }], vec![])
    }

    fn make_bids(&self, request: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(request.imp.len());
        if let Some(cur) = &bid_resp.cur {
            result.currency = cur.clone();
        }

        let mut errors = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let mtype = bid.mtype.unwrap_or(0);
                match get_bid_type_from_mtype(mtype) {
                    Ok(bid_type) => result.bids.push(TypedBid::new(bid, bid_type)),
                    Err(e) => errors.push(e),
                }
            }
        }

        if !errors.is_empty() {
            return Err(errors);
        }
        if result.bids.is_empty() {
            return Err(vec![BidderError::BadServerResponse("no bids returned".to_string())]);
        }

        Ok(result)
    }
}
