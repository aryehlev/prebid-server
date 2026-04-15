use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct RiseAdapter { pub endpoint: String }
impl RiseAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

fn extract_org(request: &openrtb::BidRequest) -> Result<String, BidderError> {
    for imp in &request.imp {
        let bidder = imp.ext.as_ref().and_then(|e| e.get("bidder"));
        let org = bidder.and_then(|b| b.get("org")).and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
        if !org.is_empty() { return Ok(org); }
        let pub_id = bidder.and_then(|b| b.get("publisher_id")).and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
        if !pub_id.is_empty() { return Ok(pub_id); }
    }
    Err(BidderError::BadInput("no org or publisher_id supplied".to_string()))
}

fn get_media_type_for_bid(bid: &openrtb::Bid) -> Result<BidType, BidderError> {
    match bid.mtype.unwrap_or(0) {
        1 => Ok(BidType::Banner),
        2 => Ok(BidType::Video),
        4 => Ok(BidType::Native),
        mtype => Err(BidderError::BadServerResponse(format!("unsupported MType {}", mtype))),
    }
}

impl Bidder for RiseAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let org = match extract_org(request) {
            Ok(o) => o,
            Err(e) => return (vec![], vec![e]),
        };
        let body = match serde_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        let uri = format!("{}?publisher_id={}", self.endpoint, org);
        (vec![RequestData { method: "POST".to_string(), uri, body, headers, imp_ids: get_imp_ids(&request.imp) }], vec![])
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(internal.imp.len());
        if let Some(cur) = &bid_resp.cur { if !cur.is_empty() { result.currency = cur.clone(); } }
        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                match get_media_type_for_bid(&bid) {
                    Ok(bid_type) => result.bids.push(TypedBid::new(bid, bid_type)),
                    Err(e) => errs.push(e),
                }
            }
        }
        // Return bids along with any errors (Go behavior: return bidResponse, errs)
        if !errs.is_empty() && result.bids.is_empty() {
            return Err(errs);
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
                banner: Some(openrtb::Banner {
                    w: Some(300),
                    h: Some(250),
                    ..Default::default()
                }),
                ext: Some(serde_json::json!({"bidder": {"org": "pub1"}})),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn test_rise_url_and_headers() {
        let adapter = RiseAdapter::new("https://pbs.yellowblue.io/pbs".to_string());
        let (reqs, errs) = adapter.make_requests(&make_req(), &ExtraRequestInfo::default());
        assert!(errs.is_empty(), "errs: {:?}", errs);
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].uri, "https://pbs.yellowblue.io/pbs?publisher_id=pub1");
        assert_eq!(
            reqs[0].headers.get("Content-Type").unwrap(),
            "application/json;charset=utf-8"
        );
        assert!(!reqs[0].body.is_empty());
    }

    #[test]
    fn test_rise_make_bids() {
        let adapter = RiseAdapter::new("https://pbs.yellowblue.io/pbs".to_string());
        let body = br#"{"id":"r","seatbid":[{"bid":[{"id":"b","impid":"i1","price":1.3,"crid":"c","mtype":1}]}]}"#;
        let resp = ResponseData::new(200, body.to_vec());
        let result = adapter
            .make_bids(&make_req(), &RequestData::default(), &resp)
            .unwrap();
        assert_eq!(result.bids.len(), 1);
        assert_eq!(result.bids[0].bid_type, BidType::Banner);
    }
}
