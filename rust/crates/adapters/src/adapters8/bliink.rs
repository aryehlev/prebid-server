use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct BliinkAdapter {
    pub endpoint: String,
}

impl BliinkAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

fn get_media_type_for_imp(imp_id: &str, imps: &[openrtb::Imp]) -> Result<BidType, BidderError> {
    let mut media_type = BidType::Banner;
    let mut type_count = 0;
    for imp in imps {
        if imp.id == imp_id {
            if imp.banner.is_some() { type_count += 1; media_type = BidType::Banner; }
            if imp.native.is_some() { type_count += 1; media_type = BidType::Native; }
            if imp.video.is_some() { type_count += 1; media_type = BidType::Video; }
        }
    }
    if type_count == 1 {
        Ok(media_type)
    } else {
        Err(BidderError::BadServerResponse(format!(
            "unable to fetch mediaType in multi-format: {}", imp_id
        )))
    }
}

impl Bidder for BliinkAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let body = match serde_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
        headers.insert("x-openrtb-version".to_string(), "2.5".to_string());

        (vec![RequestData {
            method: "POST".to_string(),
            uri: self.endpoint.clone(),
            body,
            headers,
            imp_ids: get_imp_ids(&request.imp),
        }], vec![])
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code == 400 {
            return Err(vec![BidderError::BadInput(
                "Unexpected status code: 400. Bad request from publisher. Run with request.debug = 1 for more info.".to_string()
            )]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info.", response.status_code
            ))]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        if bid_resp.seatbid.is_empty() {
            return Err(vec![BidderError::BadServerResponse("Empty SeatBid array".to_string())]);
        }

        let mut result = BidderResponse::with_capacity(internal.imp.len());
        if let Some(cur) = &bid_resp.cur {
            result.currency = cur.clone();
        }
        let mut errs = Vec::new();

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                match get_media_type_for_imp(&bid.impid, &internal.imp) {
                    Ok(bt) => result.bids.push(TypedBid::new(bid, bt)),
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

    #[test]
    fn test_make_requests_sets_headers() {
        let adapter = BliinkAdapter::new("https://bliink.example/rtb".to_string());
        let mut req = openrtb::BidRequest::default();
        req.id = "req-1".to_string();
        req.imp = vec![openrtb::Imp {
            id: "imp1".to_string(),
            banner: Some(Default::default()),
            ..Default::default()
        }];
        let info = ExtraRequestInfo::default();
        let (requests, errs) = adapter.make_requests(&req, &info);
        assert!(errs.is_empty());
        assert_eq!(requests.len(), 1);
        let r = &requests[0];
        assert_eq!(r.method, "POST");
        assert_eq!(r.uri, "https://bliink.example/rtb");
        assert_eq!(r.headers.get("x-openrtb-version").map(String::as_str), Some("2.5"));
        assert_eq!(r.imp_ids, vec!["imp1".to_string()]);
    }
}
