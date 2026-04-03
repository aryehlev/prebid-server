use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use openrtb_ext::BidType;

pub struct KueezrtbAdapter { pub endpoint: String }
impl KueezrtbAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

fn get_bid_type_from_mtype(mtype: i32) -> Result<BidType, BidderError> {
    match mtype {
        1 => Ok(BidType::Banner),
        2 => Ok(BidType::Video),
        _ => Err(BidderError::BadInput(
            format!("Could not define bid type for imp with mtype {}", mtype)
        )),
    }
}

fn url_query_escape(s: &str) -> String {
    // percent-encode the string for use in URL query
    let mut encoded = String::new();
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => {
                encoded.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    encoded
}

impl Bidder for KueezrtbAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();
        let mut errs = Vec::new();

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());

        for imp in &request.imp {
            // Extract connectionId from imp.ext.bidder
            let bidder = imp.ext.as_ref()
                .and_then(|e| e.get("bidder"))
                .cloned()
                .unwrap_or(serde_json::Value::Null);

            let connection_id = bidder.get("cId")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            if connection_id.is_empty() {
                errs.push(BidderError::BadInput(format!("extract cId: missing for imp {}", imp.id)));
                continue;
            }

            let url = format!("{}{}", self.endpoint, url_query_escape(connection_id));

            let mut req_copy = request.clone();
            req_copy.imp = vec![imp.clone()];

            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => { errs.push(BidderError::BadInput(format!("marshal bidRequest: {}", e))); continue; }
            };

            requests.push(RequestData {
                method: "POST".to_string(),
                uri: url,
                body,
                headers: headers.clone(),
                imp_ids: vec![imp.id.clone()],
            });
        }

        (requests, errs)
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(_) = crate::check_response_status(response.status_code) {
            return Err(vec![BidderError::BadInput(
                format!("Unexpected status code: {}. Run with request.debug = 1 for more info", response.status_code)
            )]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(
                format!("bad server response: {}. ", e)
            )])?;

        let mut result = BidderResponse::with_capacity(bid_resp.seatbid.len());
        if let Some(cur) = &bid_resp.cur {
            if !cur.is_empty() {
                result.currency = cur.clone();
            }
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
        Ok(result)
    }
}
