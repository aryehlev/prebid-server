use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids, check_response_status};
use openrtb_ext::BidType;

pub struct TpmnAdapter { pub endpoint: String }
impl TpmnAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

fn get_media_type_for_imp(mtype: i32, imp_id: &str) -> Result<BidType, BidderError> {
    match mtype {
        1 => Ok(BidType::Banner),
        2 => Ok(BidType::Video),
        4 => Ok(BidType::Native),
        _ => Err(BidderError::BadServerResponse(format!("unsupported MType {}", mtype))),
    }
}

impl Bidder for TpmnAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        // Normalize bid floor currency to USD
        let mut req_copy = request.clone();
        let mut errs = Vec::new();
        let mut valid_imps = Vec::new();
        for mut imp in req_copy.imp.clone() {
            // Skip currency conversion (no convert_currency available), just set USD
            if imp.bidfloor.unwrap_or(0.0) > 0.0 {
                let cur = imp.bidfloorcur.clone().unwrap_or_default().to_uppercase();
                if !cur.is_empty() && cur != "USD" {
                    // Cannot convert - skip this imp
                    errs.push(BidderError::BadInput(format!(
                        "cannot convert bid floor currency {} to USD", cur
                    )));
                    continue;
                }
            }
            imp.bidfloorcur = Some("USD".to_string());
            valid_imps.push(imp);
        }
        if valid_imps.is_empty() {
            return (vec![], errs);
        }
        req_copy.imp = valid_imps;

        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => return (vec![], { errs.push(BidderError::BadInput(e.to_string())); errs }),
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

    fn make_bids(&self, request: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(format!("bid response unmarshal: {}", e))])?;
        let mut result = BidderResponse::with_capacity(request.imp.len());
        if let Some(cur) = &bid_resp.cur {
            result.currency = cur.clone();
        }
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let mtype = bid.mtype.unwrap_or(0);
                match get_media_type_for_imp(mtype, &bid.impid) {
                    Ok(t) => result.bids.push(TypedBid::new(bid, t)),
                    Err(e) => return Err(vec![e]),
                }
            }
        }
        Ok(result)
    }
}
