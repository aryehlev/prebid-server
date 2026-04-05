use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;

pub struct AaxAdapter { pub endpoint: String }
impl AaxAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

#[derive(Deserialize, Default)]
struct AaxResponseBidExt {
    #[serde(rename = "adCodeType", default)]
    ad_code_type: String,
}

fn get_aax_bid_type(bid: &openrtb::Bid, imps: &[openrtb::Imp]) -> Result<BidType, BidderError> {
    // First try ext.adCodeType
    if let Some(ext) = &bid.ext {
        if let Ok(bid_ext) = serde_json::from_value::<AaxResponseBidExt>(ext.clone()) {
            match bid_ext.ad_code_type.as_str() {
                "banner" => return Ok(BidType::Banner),
                "native" => return Ok(BidType::Native),
                "video" => return Ok(BidType::Video),
                _ => {}
            }
        }
    }
    // Fallback: find the matching imp and use its type
    let mut media_type = BidType::Banner;
    let mut type_cnt = 0;
    for imp in imps {
        if imp.id == bid.impid {
            if imp.banner.is_some() { type_cnt += 1; media_type = BidType::Banner; }
            if imp.native.is_some() { type_cnt += 1; media_type = BidType::Native; }
            if imp.video.is_some() { type_cnt += 1; media_type = BidType::Video; }
        }
    }
    if type_cnt == 1 {
        Ok(media_type)
    } else {
        Err(BidderError::BadServerResponse(format!("unable to fetch mediaType in multi-format: {}", bid.impid)))
    }
}

impl Bidder for AaxAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let body = match serde_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        (vec![RequestData { method: "POST".to_string(), uri: self.endpoint.clone(), body, headers, imp_ids: get_imp_ids(&request.imp) }], vec![])
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        match response.status_code {
            200 => {}
            204 => return Ok(BidderResponse::new()),
            400 => return Err(vec![BidderError::BadInput(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info",
                response.status_code
            ))]),
            _ => return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info",
                response.status_code
            ))]),
        }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(5);
        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                match get_aax_bid_type(&bid, &internal.imp) {
                    Ok(bt) => result.bids.push(TypedBid::new(bid, bt)),
                    Err(e) => errs.push(e),
                }
            }
        }
        // Return accumulated bids; bid-type errors are non-fatal
        Ok(result)
    }
}
