use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct BlueAdapter {
    pub endpoint: String,
}

impl BlueAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

impl Bidder for BlueAdapter {
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

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        match response.status_code {
            204 => return Ok(BidderResponse::new()),
            200 => {}
            400 => return Err(vec![BidderError::BadInput(format!(
                "Unexpected status code: {}.", response.status_code
            ))]),
            _ => return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected status code: {}.", response.status_code
            ))]),
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(internal.imp.len());
        if let Some(cur) = &bid_resp.cur {
            result.currency = cur.clone();
        }

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                result.bids.push(TypedBid::new(bid, BidType::Banner));
            }
        }

        Ok(result)
    }
}
