use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;

pub struct ColossusAdapter { pub endpoint: String }
impl ColossusAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

#[derive(Debug, Default, Deserialize)]
struct ColossusResponseBidExt {
    #[serde(rename = "mediaType", default)]
    media_type: String,
}

fn get_media_type_for_imp(bid: &openrtb::Bid, imps: &[openrtb::Imp]) -> Result<BidType, BidderError> {
    // First try bid ext
    if let Some(ext) = &bid.ext {
        if let Ok(bid_ext) = serde_json::from_value::<ColossusResponseBidExt>(ext.clone()) {
            match bid_ext.media_type.as_str() {
                "banner" => return Ok(BidType::Banner),
                "native" => return Ok(BidType::Native),
                "video" => return Ok(BidType::Video),
                _ => {}
            }
        }
    }
    // Fall back to imp type
    for imp in imps {
        if imp.id == bid.impid {
            if imp.banner.is_some() {
                return Ok(BidType::Banner);
            }
            if imp.video.is_some() {
                return Ok(BidType::Video);
            }
            if imp.native.is_some() {
                return Ok(BidType::Native);
            }
        }
    }
    Err(BidderError::BadInput(format!("Failed to find impression \"{}\"", bid.impid)))
}

impl Bidder for ColossusAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut adapter_requests = Vec::new();

        for imp in &request.imp {
            let mut req_copy = request.clone();
            let mut imp_copy = imp.clone();

            // Extract TagID from bidder ext
            if let Some(ext) = &imp_copy.ext {
                if let Some(bidder) = ext.get("bidder") {
                    if let Some(tag_id) = bidder.get("TagID").and_then(|v| v.as_str()) {
                        imp_copy.tagid = Some(tag_id.to_string());
                    }
                }
            }

            req_copy.imp = vec![imp_copy];

            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(BidderError::BadInput(e.to_string()));
                    continue;
                }
            };

            let mut headers = HashMap::new();
            headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
            headers.insert("Accept".to_string(), "application/json".to_string());

            let imp_ids = get_imp_ids(&req_copy.imp);
            adapter_requests.push(RequestData {
                method: "POST".to_string(),
                uri: self.endpoint.clone(),
                body,
                headers,
                imp_ids,
            });
        }

        (adapter_requests, errs)
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(1);
        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                match get_media_type_for_imp(&bid, &internal.imp) {
                    Ok(bid_type) => result.bids.push(TypedBid::new(bid, bid_type)),
                    Err(e) => errs.push(e),
                }
            }
        }
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
                banner: Some(openrtb::Banner { w: Some(300), h: Some(250), ..Default::default() }),
                ext: Some(serde_json::json!({"bidder": {"TagID": "tag123"}})),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn test_make_requests_basic() {
        let adapter = ColossusAdapter::new("https://colossus.example.com/bid".to_string());
        let (reqs, errs) = adapter.make_requests(&make_req(), &ExtraRequestInfo::default());
        assert!(errs.is_empty());
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].uri, "https://colossus.example.com/bid");
        assert!(reqs[0].headers.contains_key("Content-Type"));
        assert!(!reqs[0].body.is_empty());
    }

    #[test]
    fn test_make_bids_basic() {
        let adapter = ColossusAdapter::new("https://c".to_string());
        let body = br#"{"id":"r","seatbid":[{"bid":[{"id":"b1","impid":"i1","price":1.0,"ext":{"mediaType":"banner"}}]}]}"#;
        let resp = ResponseData::new(200, body.to_vec());
        let result = adapter.make_bids(&make_req(), &RequestData::default(), &resp).unwrap();
        assert_eq!(result.bids.len(), 1);
        assert_eq!(result.bids[0].bid_type, BidType::Banner);
    }
}
