use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use openrtb_ext::BidType;
use serde::{Deserialize, Serialize};

pub struct MgidXAdapter { pub endpoint: String }
impl MgidXAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

#[derive(Debug, Deserialize, Default)]
struct ImpExtMgidX {
    #[serde(rename = "placementId", default)]
    placement_id: String,
    #[serde(rename = "endpointId", default)]
    endpoint_id: String,
}

#[derive(Debug, Serialize)]
struct ReqBodyExtBidder {
    #[serde(rename = "type")]
    kind: String,
    #[serde(rename = "placementId", skip_serializing_if = "String::is_empty")]
    placement_id: String,
    #[serde(rename = "endpointId", skip_serializing_if = "String::is_empty")]
    endpoint_id: String,
}

#[derive(Debug, Serialize)]
struct ReqBodyExt {
    bidder: ReqBodyExtBidder,
}

fn get_bid_media_type(bid: &openrtb::Bid) -> Result<BidType, BidderError> {
    let t = bid.ext.as_ref()
        .and_then(|e| e.get("prebid"))
        .and_then(|p| p.get("type"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if t.is_empty() {
        return Err(BidderError::BadServerResponse(format!(
            "imp {} with unknown media type", bid.impid
        )));
    }
    match t {
        "banner" => Ok(BidType::Banner),
        "video" => Ok(BidType::Video),
        "native" => Ok(BidType::Native),
        "audio" => Ok(BidType::Audio),
        _ => Err(BidderError::BadServerResponse(format!(
            "imp {} with unknown media type", bid.impid
        ))),
    }
}

impl Bidder for MgidXAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();
        let mut errs = Vec::new();
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        for imp in &request.imp {
            let bidder_val = match imp.ext.as_ref().and_then(|e| e.get("bidder")) {
                Some(v) => v.clone(),
                None => { errs.push(BidderError::BadInput("missing bidder ext".to_string())); return (vec![], errs); }
            };
            let mgid_ext: ImpExtMgidX = match serde_json::from_value(bidder_val) {
                Ok(v) => v,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); return (vec![], errs); }
            };

            let new_bidder = if !mgid_ext.placement_id.is_empty() {
                ReqBodyExtBidder {
                    kind: "publisher".to_string(),
                    placement_id: mgid_ext.placement_id,
                    endpoint_id: String::new(),
                }
            } else if !mgid_ext.endpoint_id.is_empty() {
                ReqBodyExtBidder {
                    kind: "network".to_string(),
                    placement_id: String::new(),
                    endpoint_id: mgid_ext.endpoint_id,
                }
            } else {
                continue;
            };

            let new_ext = ReqBodyExt { bidder: new_bidder };
            let new_ext_val = match serde_json::to_value(&new_ext) {
                Ok(v) => v,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); return (vec![], errs); }
            };

            let mut imp_copy = imp.clone();
            imp_copy.ext = Some(new_ext_val);

            let mut req_copy = request.clone();
            req_copy.imp = vec![imp_copy];
            let imp_ids = req_copy.imp.iter().map(|i| i.id.clone()).collect();
            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); return (vec![], errs); }
            };
            requests.push(RequestData { method: "POST".to_string(), uri: self.endpoint.clone(), body, headers: headers.clone(), imp_ids });
        }

        if requests.is_empty() && errs.is_empty() {
            errs.push(BidderError::BadInput("found no valid impressions".to_string()));
        }

        (requests, errs)
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(internal.imp.len());
        if let Some(cur) = &bid_resp.cur { if !cur.is_empty() { result.currency = cur.clone(); } }
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                match get_bid_media_type(&bid) {
                    Ok(t) => result.bids.push(TypedBid::new(bid, t)),
                    Err(e) => return Err(vec![e]),
                }
            }
        }
        Ok(result)
    }
}
