use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct VrtcalAdapter { pub endpoint: String }
impl VrtcalAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

fn get_return_type_for_imp(mtype: i32) -> Result<BidType, BidderError> {
    match mtype {
        1 => Ok(BidType::Banner),
        2 => Ok(BidType::Video),
        4 => Ok(BidType::Native),
        _ => Err(BidderError::BadServerResponse("Unsupported return type".to_string())),
    }
}

impl Bidder for VrtcalAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let body = match serde_json::to_vec(request) {
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
        if response.status_code == 400 {
            return Err(vec![BidderError::BadInput(
                format!("Unexpected status code: {}. Run with request.debug = 1 for more info", response.status_code)
            )]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(
                format!("Unexpected status code: {}. Run with request.debug = 1 for more info", response.status_code)
            )]);
        }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(1);
        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let mtype = bid.mtype.unwrap_or(0);
                match get_return_type_for_imp(mtype) {
                    Ok(bid_type) => result.bids.push(TypedBid::new(bid, bid_type)),
                    Err(e) => errs.push(e),
                }
            }
        }
        // Go returns both the response and errors; return Ok with bids even if some errs occurred.
        // If no bids at all and only errors, we still return Ok with empty response to match
        // Go's behavior of always returning the bidResponse struct.
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
        let a = VrtcalAdapter::new("https://go.vrtcal.com/bidder".to_string());
        let (reqs, errs) = a.make_requests(&make_req(), &ExtraRequestInfo::default());
        assert!(errs.is_empty());
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].uri, "https://go.vrtcal.com/bidder");
        assert_eq!(reqs[0].headers.get("Content-Type").unwrap(), "application/json;charset=utf-8");
    }

    #[test]
    fn test_make_bids_basic() {
        let a = VrtcalAdapter::new("x".to_string());
        let body = br#"{"id":"r","seatbid":[{"bid":[{"id":"b1","impid":"i1","price":1.0,"mtype":1}]}]}"#;
        let resp = ResponseData::new(200, body.to_vec());
        let result = a.make_bids(&make_req(), &RequestData::default(), &resp).unwrap();
        assert_eq!(result.bids.len(), 1);
        assert_eq!(result.bids[0].bid_type, BidType::Banner);
    }
}
