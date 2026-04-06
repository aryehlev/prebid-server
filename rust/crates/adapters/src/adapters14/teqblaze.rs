use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, check_response_status};
use openrtb_ext::BidType;
use serde::{Deserialize, Serialize};

pub struct TeqblazeAdapter { pub endpoint: String }
impl TeqblazeAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

#[derive(Debug, Deserialize)]
struct ExtImpTeqBlaze {
    #[serde(rename = "placementId", default)]
    placement_id: String,
    #[serde(rename = "endpointId", default)]
    endpoint_id: String,
}

#[derive(Serialize)]
struct ReqBodyExtBidder {
    #[serde(rename = "type")]
    type_: String,
    #[serde(rename = "placementId", skip_serializing_if = "String::is_empty")]
    placement_id: String,
    #[serde(rename = "endpointId", skip_serializing_if = "String::is_empty")]
    endpoint_id: String,
}

#[derive(Serialize)]
struct ReqBodyExt {
    bidder: ReqBodyExtBidder,
}

fn get_bid_type(mtype: i32, imp_id: &str) -> Result<BidType, BidderError> {
    match mtype {
        1 => Ok(BidType::Banner),
        2 => Ok(BidType::Video),
        4 => Ok(BidType::Native),
        _ => Err(BidderError::BadServerResponse(format!("could not define media type for bid: {}", imp_id))),
    }
}

impl Bidder for TeqblazeAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut adapter_requests = Vec::new();
        let mut errs = Vec::new();

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        for imp in &request.imp {
            let bidder_val = match imp.ext.as_ref().and_then(|e| e.get("bidder")) {
                Some(v) => v.clone(),
                None => { errs.push(BidderError::BadInput("missing bidder ext".to_string())); continue; }
            };
            let ext: ExtImpTeqBlaze = match serde_json::from_value(bidder_val) {
                Ok(e) => e,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };

            let imp_ext = if !ext.placement_id.is_empty() {
                ReqBodyExt {
                    bidder: ReqBodyExtBidder {
                        type_: "publisher".to_string(),
                        placement_id: ext.placement_id,
                        endpoint_id: String::new(),
                    },
                }
            } else {
                ReqBodyExt {
                    bidder: ReqBodyExtBidder {
                        type_: "network".to_string(),
                        placement_id: String::new(),
                        endpoint_id: ext.endpoint_id,
                    },
                }
            };

            let imp_ext_json = match serde_json::to_value(&imp_ext) {
                Ok(v) => v,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };

            let mut imp_copy = imp.clone();
            imp_copy.ext = Some(imp_ext_json);

            let mut req_copy = request.clone();
            req_copy.imp = vec![imp_copy];

            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };

            adapter_requests.push(RequestData {
                method: "POST".to_string(),
                uri: self.endpoint.clone(),
                body,
                headers: headers.clone(),
                imp_ids: vec![imp.id.clone()],
            });
        }

        (adapter_requests, errs)
    }

    fn make_bids(&self, request: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(request.imp.len());
        if let Some(cur) = &bid_resp.cur {
            if !cur.is_empty() { result.currency = cur.clone(); }
        }
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let mtype = bid.mtype.unwrap_or(0);
                match get_bid_type(mtype, &bid.impid) {
                    Ok(t) => result.bids.push(TypedBid::new(bid, t)),
                    Err(e) => return Err(vec![e]),
                }
            }
        }
        Ok(result)
    }
}
