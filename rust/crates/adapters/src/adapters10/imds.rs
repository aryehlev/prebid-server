use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_bid_type_from_imp, get_imp_ids, check_response_status};
use openrtb::BidResponse;
use openrtb_ext::BidType;

const ADAPTER_VERSION: &str = "pbs-go/1.0.0";

pub struct ImdsAdapter { pub endpoint: String }
impl ImdsAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

impl Bidder for ImdsAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut valid_imps = Vec::new();
        let mut first_seat_id: Option<String> = None;
        let mut first_tag_id: Option<String> = None;

        for imp in &request.imp {
            let bidder = imp.ext.as_ref()
                .and_then(|e| e.get("bidder"))
                .cloned()
                .unwrap_or(serde_json::Value::Null);

            let seat_id = bidder.get("seatId").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let tag_id = bidder.get("tagId").and_then(|v| v.as_str()).unwrap_or("").to_string();

            if seat_id.is_empty() || tag_id.is_empty() {
                errs.push(BidderError::BadServerResponse("Invalid Impression".to_string()));
                continue;
            }

            let mut imp_copy = imp.clone();
            imp_copy.tagid = Some(tag_id.clone());
            valid_imps.push(imp_copy);

            if first_seat_id.is_none() {
                first_seat_id = Some(seat_id);
                first_tag_id = Some(tag_id);
            }
        }

        if valid_imps.is_empty() {
            return (vec![], errs);
        }

        let seat_id = match first_seat_id {
            Some(s) => s,
            None => return (vec![], errs),
        };

        // Build request ext with seatId
        let req_ext = serde_json::json!({ "seatId": seat_id });

        let mut req = request.clone();
        req.imp = valid_imps;
        req.ext = Some(req_ext);

        let body = match serde_json::to_vec(&req) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        // Build URL: replace {{.AccountID}} with url-encoded seatId, {{.SourceId}} with adapter version
        let encoded_seat = urlencoded(&seat_id);
        let encoded_version = urlencoded(ADAPTER_VERSION);
        let url = self.endpoint
            .replace("{{.AccountID}}", &encoded_seat)
            .replace("{{.SourceId}}", &encoded_version);

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        (vec![RequestData {
            method: "POST".to_string(),
            uri: url,
            body,
            headers,
            imp_ids: get_imp_ids(&request.imp),
        }], errs)
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        match response.status_code {
            204 => return Ok(BidderResponse::new()),
            400 => return Err(vec![BidderError::BadInput(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info", response.status_code
            ))]),
            200 => {},
            _ => return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info", response.status_code
            ))]),
        }

        let bid_resp: BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(1);
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let media_type = get_bid_type_from_imp_for_id(&bid.impid, &internal.imp);
                // Only include banner and video
                if media_type == BidType::Banner || media_type == BidType::Video {
                    result.bids.push(TypedBid::new(bid, media_type));
                }
            }
        }
        Ok(result)
    }
}

fn get_bid_type_from_imp_for_id(imp_id: &str, imps: &[openrtb::Imp]) -> BidType {
    for imp in imps {
        if imp.id == imp_id {
            return get_bid_type_from_imp(imp);
        }
    }
    BidType::Banner
}

fn urlencoded(s: &str) -> String {
    let mut result = String::new();
    for c in s.chars() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => result.push(c),
            _ => {
                for byte in c.to_string().as_bytes() {
                    result.push_str(&format!("%{:02X}", byte));
                }
            }
        }
    }
    result
}
