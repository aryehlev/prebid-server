use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct StartioAdapter { pub endpoint: String }
impl StartioAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

fn is_supported_currency(cur: &Option<Vec<String>>) -> bool {
    match cur {
        None => true,
        Some(currencies) => currencies.is_empty() || currencies.iter().any(|c| c == "USD"),
    }
}

fn get_media_type_for_bid(bid: &openrtb::Bid) -> Result<BidType, BidderError> {
    if let Some(ext) = &bid.ext {
        if let Some(prebid) = ext.get("prebid") {
            if let Some(type_str) = prebid.get("type").and_then(|v| v.as_str()) {
                return match type_str {
                    "banner" => Ok(BidType::Banner),
                    "video" => Ok(BidType::Video),
                    "native" => Ok(BidType::Native),
                    _ => Err(BidderError::BadServerResponse(format!(
                        "Failed to parse bid media type for impression {}.", bid.impid
                    ))),
                };
            }
        }
    }
    Err(BidderError::BadServerResponse(format!(
        "Failed to parse bid media type for impression {}.", bid.impid
    )))
}

impl Bidder for StartioAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        if !is_supported_currency(&request.cur) {
            return (vec![], vec![BidderError::BadInput("unsupported currency: only USD is accepted".to_string())]);
        }

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
        headers.insert("X-Openrtb-Version".to_string(), "2.5".to_string());

        let mut requests = Vec::new();
        let mut errs = Vec::new();

        let impressions = request.imp.clone();
        for (i, imp) in impressions.iter().enumerate() {
            let mut req_copy = request.clone();
            req_copy.imp = vec![imp.clone()];
            match serde_json::to_vec(&req_copy) {
                Ok(body) => {
                    requests.push(RequestData {
                        method: "POST".to_string(),
                        uri: self.endpoint.clone(),
                        body,
                        headers: headers.clone(),
                        imp_ids: vec![imp.id.clone()],
                    });
                }
                Err(e) => {
                    errs.push(BidderError::BadInput(format!("imp[{}]: failed to marshal request: {}", i, e)));
                }
            }
        }

        (requests, errs)
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(format!("failed to unmarshal response body: {}", e))])?;

        if bid_resp.seatbid.is_empty() || bid_resp.seatbid[0].bid.is_empty() {
            return Ok(BidderResponse::new());
        }

        let mut result = BidderResponse::with_capacity(bid_resp.seatbid[0].bid.len());
        let mut errs = Vec::new();

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                match get_media_type_for_bid(&bid) {
                    Ok(t) => result.bids.push(TypedBid::new(bid, t)),
                    Err(e) => errs.push(e),
                }
            }
        }

        if !errs.is_empty() { return Err(errs); }
        Ok(result)
    }
}
