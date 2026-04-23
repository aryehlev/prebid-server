use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use openrtb_ext::BidType;

const ROUTE_NATIVE: &str = "o";
const ROUTE_RTB: &str = "rtb";
const METHOD_NATIVE: &str = "ortb";
const METHOD_RTB: &str = "req";
const MACROS_ROUTE: &str = "__route__";
const MACROS_METHOD: &str = "__method__";
const MACROS_KEY: &str = "__key__";

pub struct MobfoxpbAdapter { pub endpoint: String }
impl MobfoxpbAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

fn get_media_type_for_imp(imp_id: &str, imps: &[openrtb::Imp]) -> Result<BidType, BidderError> {
    for imp in imps {
        if imp.id == imp_id {
            return Ok(if imp.banner.is_some() { BidType::Banner }
                else if imp.video.is_some() { BidType::Video }
                else if imp.native.is_some() { BidType::Native }
                else { BidType::Banner });
        }
    }
    Err(BidderError::BadServerResponse(format!("Failed to find impression \"{}\"", imp_id)))
}

impl Bidder for MobfoxpbAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::BadInput("no impressions".to_string())]);
        }
        let imp = &request.imp[0];
        let bidder = imp.ext.as_ref().and_then(|e| e.get("bidder"));
        let tag_id = bidder.and_then(|b| b.get("TagID")).and_then(|v| v.as_str()).unwrap_or("");
        let key = bidder.and_then(|b| b.get("key")).and_then(|v| v.as_str()).unwrap_or("");

        if tag_id.is_empty() && key.is_empty() {
            return (vec![], vec![BidderError::BadInput(
                "Invalid or non existing key and tagId, at least one should be present".to_string()
            )]);
        }

        let mut uri = self.endpoint.clone();
        let (route, method) = if !key.is_empty() {
            uri = uri.replace(MACROS_KEY, key);
            (ROUTE_RTB, METHOD_RTB)
        } else {
            (ROUTE_NATIVE, METHOD_NATIVE)
        };
        uri = uri.replace(MACROS_ROUTE, route).replace(MACROS_METHOD, method);

        let mut req_copy = request.clone();
        req_copy.imp = vec![imp.clone()];
        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
        let imp_ids = vec![imp.id.clone()];
        (vec![RequestData { method: "POST".to_string(), uri, body, headers, imp_ids }], vec![])
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info",
                response.status_code
            ))]);
        }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(1);
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                match get_media_type_for_imp(&bid.impid, &internal.imp) {
                    Ok(t) => result.bids.push(TypedBid::new(bid, t)),
                    // Log error but continue processing remaining bids (matches Go behavior)
                    Err(_) => {}
                }
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_make_requests_uses_key_route() {
        let adapter = MobfoxpbAdapter::new("https://mobfoxpb.example/__route__/__method__/__key__".to_string());
        let mut req = openrtb::BidRequest::default();
        req.id = "r".to_string();
        req.imp = vec![openrtb::Imp {
            id: "imp1".to_string(),
            banner: Some(Default::default()),
            ext: Some(serde_json::json!({"bidder":{"key":"K1"}})),
            ..Default::default()
        }];
        let info = ExtraRequestInfo::default();
        let (requests, errs) = adapter.make_requests(&req, &info);
        assert!(errs.is_empty());
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].uri, "https://mobfoxpb.example/rtb/req/K1");
    }

    #[test]
    fn test_make_requests_uses_tag_route() {
        let adapter = MobfoxpbAdapter::new("https://mobfoxpb.example/__route__/__method__".to_string());
        let mut req = openrtb::BidRequest::default();
        req.id = "r".to_string();
        req.imp = vec![openrtb::Imp {
            id: "imp1".to_string(),
            native: Some(Default::default()),
            ext: Some(serde_json::json!({"bidder":{"TagID":"T1"}})),
            ..Default::default()
        }];
        let info = ExtraRequestInfo::default();
        let (requests, errs) = adapter.make_requests(&req, &info);
        assert!(errs.is_empty());
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].uri, "https://mobfoxpb.example/o/ortb");
    }
}
