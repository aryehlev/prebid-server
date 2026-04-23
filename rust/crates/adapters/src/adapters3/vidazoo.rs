use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use openrtb_ext::BidType;
use serde::Deserialize;

pub struct VidazooAdapter {
    pub endpoint: String,
}

impl VidazooAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

/// Extension from imp.ext.bidder for Vidazoo
#[derive(Debug, Default, Deserialize)]
struct ImpExtVidazoo {
    #[serde(rename = "cId", default)]
    connection_id: String,
}

fn extract_cid(imp: &openrtb::Imp) -> Result<String, BidderError> {
    let bidder_val = imp.ext.as_ref()
        .and_then(|e| e.get("bidder"))
        .cloned()
        .ok_or_else(|| BidderError::BadInput("unmarshal bidderExt: missing bidder ext".to_string()))?;

    let imp_ext: ImpExtVidazoo = serde_json::from_value(bidder_val)
        .map_err(|e| BidderError::BadInput(format!("unmarshal ImpExtVidazoo: {}", e)))?;

    Ok(imp_ext.connection_id.trim().to_string())
}

fn get_media_type_for_bid(bid: &openrtb::Bid) -> Result<BidType, BidderError> {
    // OpenRTB mtype: 1=Banner, 2=Video (top-level field on Bid)
    match bid.mtype.unwrap_or(0) {
        1 => Ok(BidType::Banner),
        2 => Ok(BidType::Video),
        _ => Err(BidderError::BadInput(format!(
            "Could not define bid type for imp: {}",
            bid.impid
        ))),
    }
}

/// URL-encode a string using Go's url.QueryEscape semantics: space -> '+',
/// unreserved chars pass through, everything else percent-encoded.
fn percent_encode(s: &str) -> String {
    let mut encoded = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9'
            | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            b' ' => encoded.push('+'),
            b => {
                encoded.push_str(&format!("%{:02X}", b));
            }
        }
    }
    encoded
}

impl Bidder for VidazooAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();
        let mut errors = Vec::new();

        let mut req_copy = request.clone();

        for imp in &request.imp {
            req_copy.imp = vec![imp.clone()];

            let request_json = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => {
                    errors.push(BidderError::BadInput(format!("marshal bidRequest: {}", e)));
                    continue;
                }
            };

            let cid = match extract_cid(imp) {
                Ok(c) => c,
                Err(e) => {
                    errors.push(BidderError::BadInput(format!("extract cId: {}", e)));
                    continue;
                }
            };

            let mut headers = HashMap::new();
            headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());

            requests.push(RequestData {
                method: "POST".to_string(),
                uri: format!("{}{}", self.endpoint, percent_encode(&cid)),
                body: request_json,
                headers,
                imp_ids: vec![imp.id.clone()],
            });
        }

        (requests, errors)
    }

    fn make_bids(
        &self,
        _internal: &openrtb::BidRequest,
        _external: &RequestData,
        response: &ResponseData,
    ) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 {
            return Ok(BidderResponse::new());
        }
        // Go wraps all non-200 status codes (after 204 check) as BadInput with the message
        // "Unexpected status code: %d. Run with request.debug = 1 for more info"
        if response.status_code != 200 {
            return Err(vec![BidderError::BadInput(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info",
                response.status_code
            ))]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(format!(
                "bad server response: {}. ",
                e
            ))])?;

        let mut result = BidderResponse::with_capacity(bid_resp.seatbid.len());
        if let Some(cur) = &bid_resp.cur {
            if !cur.is_empty() {
                result.currency = cur.clone();
            }
        }

        // Go returns partial results with errors (both bids and errs), so we collect errors
        // and still return Ok with whatever bids we successfully processed.
        let mut errors = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                match get_media_type_for_bid(&bid) {
                    Ok(bid_type) => result.bids.push(TypedBid::new(bid, bid_type)),
                    Err(e) => errors.push(e),
                }
            }
        }

        // If we have errors but also have bids, we still return Ok with the bids.
        // This matches Go behavior where bids and errors are returned together.
        // If there are ONLY errors and no bids, also return Ok (Go doesn't fail the whole response).
        if !errors.is_empty() && result.bids.is_empty() {
            return Err(errors);
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
                ext: Some(serde_json::json!({"bidder": {"cId": "con123"}})),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn test_make_requests_basic() {
        let a = VidazooAdapter::new("https://prebid-server.cootlogix.com/openrtb/".to_string());
        let (reqs, errs) = a.make_requests(&make_req(), &ExtraRequestInfo::default());
        assert!(errs.is_empty());
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].uri, "https://prebid-server.cootlogix.com/openrtb/con123");
        assert_eq!(reqs[0].headers.get("Content-Type").unwrap(), "application/json;charset=utf-8");
    }

    #[test]
    fn test_make_bids_video() {
        let a = VidazooAdapter::new("x".to_string());
        let body = br#"{"id":"r","cur":"USD","seatbid":[{"bid":[{"id":"b1","impid":"i1","price":1.0,"mtype":2}]}]}"#;
        let resp = ResponseData::new(200, body.to_vec());
        let result = a.make_bids(&make_req(), &RequestData::default(), &resp).unwrap();
        assert_eq!(result.bids.len(), 1);
        assert_eq!(result.bids[0].bid_type, BidType::Video);
        assert_eq!(result.currency, "USD");
    }
}
