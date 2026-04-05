use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;
use serde_json::Value;

pub struct AdponeAdapter { pub endpoint: String }
impl AdponeAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

#[derive(Deserialize)]
struct ExtImpBidder { bidder: Value }

#[allow(dead_code)]
#[derive(Deserialize)]
struct ExtAdpone {
    #[serde(rename = "placementId", default)]
    placement_id: String,
}

impl Bidder for AdponeAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();

        // Validate the first impression's ext (mirrors Go's validation block).
        // Errors here are non-fatal — the request is still sent if imps exist.
        if let Some(imp) = request.imp.first() {
            match imp.ext.as_ref().and_then(|e| serde_json::from_value::<ExtImpBidder>(e.clone()).ok()) {
                None => errs.push(BidderError::BadInput("ext.bidder not provided".to_string())),
                Some(bidder_ext) => {
                    if serde_json::from_value::<ExtAdpone>(bidder_ext.bidder).is_err() {
                        errs.push(BidderError::BadInput("ext.bidder not provided".to_string()));
                    }
                }
            }
        }

        if request.imp.is_empty() {
            errs.push(BidderError::BadInput("No impression in the bid request".to_string()));
            return (vec![], errs);
        }

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
        }], errs)
    }

    fn make_bids(
        &self,
        _: &openrtb::BidRequest,
        _: &RequestData,
        response: &ResponseData,
    ) -> Result<BidderResponse, Vec<BidderError>> {
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

        // Compute capacity from first seat bid like Go does, but guard against empty.
        let cap = bid_resp.seatbid.first().map(|sb| sb.bid.len()).unwrap_or(0);
        let mut result = BidderResponse::with_capacity(cap);

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                result.bids.push(TypedBid::new(bid, BidType::Banner));
            }
        }

        Ok(result)
    }
}
