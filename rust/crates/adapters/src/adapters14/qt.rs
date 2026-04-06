use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids, get_bid_type_from_mtype};
use serde::{Deserialize, Serialize};

pub struct QtAdapter { pub endpoint: String }
impl QtAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

#[derive(Debug, Deserialize)]
struct ImpExtQT {
    #[serde(rename = "placementId", default)]
    placement_id: String,
    #[serde(rename = "endpointId", default)]
    endpoint_id: String,
}

#[derive(Debug, Serialize)]
struct ReqBodyExtBidder {
    #[serde(rename = "type")]
    type_: String,
    #[serde(rename = "placementId", skip_serializing_if = "String::is_empty")]
    placement_id: String,
    #[serde(rename = "endpointId", skip_serializing_if = "String::is_empty")]
    endpoint_id: String,
}

#[derive(Debug, Serialize)]
struct ReqBodyExt {
    bidder: ReqBodyExtBidder,
}

impl Bidder for QtAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        let mut adapter_requests = Vec::new();
        let mut errs = Vec::new();

        for imp in &request.imp {
            // Parse bidder ext
            let bidder_val = match imp.ext.as_ref().and_then(|e| e.get("bidder")) {
                Some(v) => v.clone(),
                None => {
                    errs.push(BidderError::BadInput("missing bidder ext".to_string()));
                    continue;
                }
            };

            let qt_ext: ImpExtQT = match serde_json::from_value(bidder_val) {
                Ok(e) => e,
                Err(err) => { errs.push(BidderError::BadInput(err.to_string())); continue; }
            };

            // Build new bidder ext
            let imp_ext = if !qt_ext.placement_id.is_empty() {
                ReqBodyExt {
                    bidder: ReqBodyExtBidder {
                        type_: "publisher".to_string(),
                        placement_id: qt_ext.placement_id,
                        endpoint_id: String::new(),
                    }
                }
            } else if !qt_ext.endpoint_id.is_empty() {
                ReqBodyExt {
                    bidder: ReqBodyExtBidder {
                        type_: "network".to_string(),
                        placement_id: String::new(),
                        endpoint_id: qt_ext.endpoint_id,
                    }
                }
            } else {
                ReqBodyExt {
                    bidder: ReqBodyExtBidder {
                        type_: String::new(),
                        placement_id: String::new(),
                        endpoint_id: String::new(),
                    }
                }
            };

            let new_ext = match serde_json::to_value(&imp_ext) {
                Ok(v) => v,
                Err(err) => { errs.push(BidderError::BadInput(err.to_string())); continue; }
            };

            let mut req_copy = request.clone();
            let mut imp_copy = imp.clone();
            imp_copy.ext = Some(new_ext);
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
                imp_ids: get_imp_ids(&req_copy.imp),
            });
        }

        (adapter_requests, errs)
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(5);
        if let Some(cur) = &bid_resp.cur { if !cur.is_empty() { result.currency = cur.clone(); } }
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let mtype = bid.mtype.unwrap_or(0);
                if mtype == 0 {
                    return Err(vec![BidderError::BadServerResponse(
                        format!("could not define media type for impression: {}", bid.impid)
                    )]);
                }
                result.bids.push(TypedBid::new(bid, get_bid_type_from_mtype(mtype)));
            }
        }
        Ok(result)
    }
}
