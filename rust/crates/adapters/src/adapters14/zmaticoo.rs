use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_bid_type_from_mtype, get_imp_ids};

pub struct ZmaticooAdapter { pub endpoint: String }
impl ZmaticooAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

/// Validate that each imp has non-empty pubId and zoneId in the bidder ext.
fn validate_zmaticoo_ext(request: &openrtb::BidRequest) -> Vec<BidderError> {
    let mut errs = Vec::new();
    for imp in &request.imp {
        let bidder = imp.ext.as_ref().and_then(|e| e.get("bidder"));
        let pub_id = bidder
            .and_then(|b| b.get("pubId"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let zone_id = bidder
            .and_then(|b| b.get("zoneId"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if pub_id.is_empty() || zone_id.is_empty() {
            errs.push(BidderError::BadInput("imp.ext.pubId or imp.ext.zoneId required".to_string()));
        }
    }
    errs
}

/// Wrap native request in a `{"native": ...}` envelope if the `native` key is missing.
fn transform_native(request: &mut openrtb::BidRequest) -> Result<(), BidderError> {
    for imp in &mut request.imp {
        if let Some(native) = &imp.native {
            let request_str = match &native.request {
                Some(s) if !s.is_empty() => s.clone(),
                _ => continue,
            };
            // Parse the native request JSON
            let native_req: serde_json::Value = serde_json::from_str(&request_str)
                .map_err(|e| BidderError::BadInput(e.to_string()))?;
            // If already wrapped with "native" key, skip
            if native_req.get("native").is_some() {
                continue;
            }
            // Wrap it
            let wrapped = serde_json::json!({ "native": native_req });
            let wrapped_str = serde_json::to_string(&wrapped)
                .map_err(|e| BidderError::BadInput(e.to_string()))?;
            let mut native_copy = native.clone();
            native_copy.request = Some(wrapped_str);
            imp.native = Some(native_copy);
        }
    }
    Ok(())
}

impl Bidder for ZmaticooAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        // Validate ext fields
        let val_errs = validate_zmaticoo_ext(request);
        if !val_errs.is_empty() {
            return (vec![], val_errs);
        }

        let mut req_copy = request.clone();
        // Transform native requests
        if let Err(e) = transform_native(&mut req_copy) {
            return (vec![], vec![e]);
        }

        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        (vec![RequestData {
            method: "POST".to_string(),
            uri: self.endpoint.clone(),
            body,
            headers,
            imp_ids: get_imp_ids(&request.imp),
        }], vec![])
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) {
            return Err(vec![BidderError::BadInput(format!("Unexpected status code: {}.", response.status_code))]);
        }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(5);
        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let mtype = bid.mtype.unwrap_or(0);
                if mtype == 0 {
                    errs.push(BidderError::BadServerResponse(format!(
                        "unrecognized bid type in response from zmaticoo for bid {}", bid.impid
                    )));
                    continue;
                }
                result.bids.push(TypedBid::new(bid, get_bid_type_from_mtype(mtype)));
            }
        }
        if !errs.is_empty() && result.bids.is_empty() { return Err(errs); }
        Ok(result)
    }
}
