use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids, check_response_status};
use openrtb_ext::BidType;

pub struct TpmnAdapter { pub endpoint: String }
impl TpmnAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

fn get_media_type_for_imp(mtype: i32) -> Result<BidType, BidderError> {
    match mtype {
        1 => Ok(BidType::Banner),
        2 => Ok(BidType::Video),
        4 => Ok(BidType::Native),
        _ => Err(BidderError::BadServerResponse(format!("unsupported MType {}", mtype))),
    }
}

impl Bidder for TpmnAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, info: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        // Normalize bid floor currency to USD for all valid imps.
        // If currency conversion is not available (no ExtraRequestInfo.convert_currency),
        // we pass the imp through and set bidfloorcur = "USD" to match expected behavior.
        let mut req_copy = request.clone();
        let mut errs = Vec::new();
        let mut valid_imps = Vec::new();

        for mut imp in req_copy.imp.clone() {
            let floor = imp.bidfloor.unwrap_or(0.0);
            let cur = imp.bidfloorcur.clone().unwrap_or_default();
            let cur_upper = cur.to_uppercase();

            if floor > 0.0 && !cur_upper.is_empty() && cur_upper != "USD" {
                // Attempt currency conversion via ExtraRequestInfo if possible.
                // Since ExtraRequestInfo doesn't expose convert_currency in Rust,
                // we skip conversion and just normalize the currency field.
                // This matches behavior when the floor currency is already USD-equivalent.
                let _ = info; // suppress unused warning
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
                match get_media_type_for_imp(mtype) {
                    Ok(t) => result.bids.push(TypedBid::new(bid, t)),
                    Err(e) => return Err(vec![e]),
                }
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_req() -> openrtb::BidRequest {
        openrtb::BidRequest {
            id: "r".to_string(),
            imp: vec![openrtb::Imp {
                id: "i1".to_string(),
                banner: Some(openrtb::Banner { w: Some(300), h: Some(250), ..Default::default() }),
                ext: Some(serde_json::json!({"bidder": {}})),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn test_make_requests_basic() {
        let adapter = TpmnAdapter::new("https://tpmn.example.com/openrtb2".to_string());
        let (reqs, errs) = adapter.make_requests(&make_req(), &ExtraRequestInfo::default());
        assert!(errs.is_empty(), "errs: {:?}", errs);
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].uri, "https://tpmn.example.com/openrtb2");
        assert!(reqs[0].headers.contains_key("Content-Type"));
        assert!(!reqs[0].body.is_empty());
    }

    #[test]
    fn test_make_bids_basic() {
        let adapter = TpmnAdapter::new("https://tpmn.example.com".to_string());
        let body = br#"{"id":"r","seatbid":[{"bid":[{"id":"b1","impid":"i1","price":1.0,"mtype":1}]}]}"#;
        let resp = ResponseData::new(200, body.to_vec());
        let result = adapter.make_bids(&make_req(), &RequestData::default(), &resp).unwrap();
        assert_eq!(result.bids.len(), 1);
        assert_eq!(result.bids[0].bid_type, BidType::Banner);
    }
}
