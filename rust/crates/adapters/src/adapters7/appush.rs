use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Mirrors Go's getMediaTypeForImp: banner > video > native priority, BadInput error if not found.
fn get_media_type_for_imp(imp_id: &str, imps: &[openrtb::Imp]) -> Result<BidType, BidderError> {
    for imp in imps {
        if imp.id == imp_id {
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
    Err(BidderError::BadInput(format!("Failed to find impression \"{}\"", imp_id)))
}

pub struct AppushAdapter {
    pub endpoint: String,
}

impl AppushAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

#[derive(Deserialize)]
struct ExtImpBidder {
    bidder: Value,
}

#[derive(Deserialize)]
struct ImpExtAppush {
    #[serde(rename = "placementId", default)]
    placement_id: String,
    #[serde(rename = "endpointId", default)]
    endpoint_id: String,
}

#[derive(Serialize)]
struct ReqBodyExtBidder {
    #[serde(rename = "type")]
    typ: String,
    #[serde(rename = "placementId", skip_serializing_if = "String::is_empty")]
    placement_id: String,
    #[serde(rename = "endpointId", skip_serializing_if = "String::is_empty")]
    endpoint_id: String,
}

#[derive(Serialize)]
struct ReqBodyExt {
    bidder: ReqBodyExtBidder,
}

impl Bidder for AppushAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        let mut results = Vec::new();
        let mut errs = Vec::new();

        for imp in &request.imp {
            let ext_val = match &imp.ext {
                Some(v) => v.clone(),
                None => { errs.push(BidderError::BadInput("missing imp ext".to_string())); continue; }
            };

            let bidder_ext: ExtImpBidder = match serde_json::from_value(ext_val) {
                Ok(v) => v,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); return (results, errs); }
            };

            let appush_ext: ImpExtAppush = match serde_json::from_value(bidder_ext.bidder) {
                Ok(v) => v,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); return (results, errs); }
            };

            let ext_bidder = if !appush_ext.placement_id.is_empty() {
                ReqBodyExtBidder {
                    typ: "publisher".to_string(),
                    placement_id: appush_ext.placement_id,
                    endpoint_id: String::new(),
                }
            } else if !appush_ext.endpoint_id.is_empty() {
                ReqBodyExtBidder {
                    typ: "network".to_string(),
                    placement_id: String::new(),
                    endpoint_id: appush_ext.endpoint_id,
                }
            } else {
                ReqBodyExtBidder {
                    typ: String::new(),
                    placement_id: String::new(),
                    endpoint_id: String::new(),
                }
            };

            let final_ext = match serde_json::to_value(ReqBodyExt { bidder: ext_bidder }) {
                Ok(v) => v,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); return (results, errs); }
            };

            let mut imp_copy = imp.clone();
            imp_copy.ext = Some(final_ext);

            let mut req_copy = request.clone();
            req_copy.imp = vec![imp_copy];

            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); return (results, errs); }
            };

            results.push(RequestData {
                method: "POST".to_string(),
                uri: self.endpoint.clone(),
                body,
                headers: headers.clone(),
                imp_ids: get_imp_ids(&req_copy.imp),
            });
        }

        (results, errs)
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info.", response.status_code
            ))]);
        }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(internal.imp.len());
        if let Some(cur) = &bid_resp.cur {
            result.currency = cur.clone();
        }
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                // Go checks banner > video > native and returns BadInput error if imp not found
                let bid_type = match get_media_type_for_imp(&bid.impid, &internal.imp) {
                    Ok(bt) => bt,
                    Err(e) => return Err(vec![e]),
                };
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        Ok(result)
    }
}
