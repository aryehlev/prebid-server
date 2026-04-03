use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::{Deserialize, Serialize};

pub struct EmtvAdapter { pub endpoint: String }
impl EmtvAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

#[derive(Debug, Default, Deserialize)]
struct ExtImpEmtv {
    #[serde(rename = "placementId", default)]
    placement_id: String,
    #[serde(rename = "endpointId", default)]
    endpoint_id: String,
}

#[derive(Debug, Default, Serialize)]
struct ReqBodyExtBidder {
    #[serde(rename = "type")]
    typ: String,
    #[serde(rename = "placementId", skip_serializing_if = "String::is_empty")]
    placement_id: String,
    #[serde(rename = "endpointId", skip_serializing_if = "String::is_empty")]
    endpoint_id: String,
}

#[derive(Debug, Default, Serialize)]
struct ReqBodyExt {
    #[serde(rename = "bidder")]
    bidder: ReqBodyExtBidder,
}

fn get_bid_type_from_imp(imp: &openrtb::Imp) -> BidType {
    if imp.banner.is_some() {
        BidType::Banner
    } else if imp.video.is_some() {
        BidType::Video
    } else if imp.native.is_some() {
        BidType::Native
    } else {
        BidType::Banner
    }
}

impl Bidder for EmtvAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut adapter_requests = Vec::new();

        for imp in &request.imp {
            // Extract bidder ext
            let bidder_val = match imp.ext.as_ref().and_then(|e| e.get("bidder")).cloned() {
                Some(v) => v,
                None => return (vec![], vec![BidderError::BadInput("missing bidder ext".to_string())]),
            };
            let emtv_ext: ExtImpEmtv = match serde_json::from_value(bidder_val) {
                Ok(e) => e,
                Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
            };

            let imp_ext = if !emtv_ext.placement_id.is_empty() {
                ReqBodyExt {
                    bidder: ReqBodyExtBidder {
                        typ: "publisher".to_string(),
                        placement_id: emtv_ext.placement_id,
                        endpoint_id: String::new(),
                    },
                }
            } else if !emtv_ext.endpoint_id.is_empty() {
                ReqBodyExt {
                    bidder: ReqBodyExtBidder {
                        typ: "network".to_string(),
                        placement_id: String::new(),
                        endpoint_id: emtv_ext.endpoint_id,
                    },
                }
            } else {
                continue;
            };

            let ext_val = match serde_json::to_value(&imp_ext) {
                Ok(v) => v,
                Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
            };

            let mut req_copy = request.clone();
            let mut imp_copy = imp.clone();
            imp_copy.ext = Some(ext_val);
            req_copy.imp = vec![imp_copy];

            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
            };

            let mut headers = HashMap::new();
            headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
            headers.insert("Accept".to_string(), "application/json".to_string());

            let imp_ids = get_imp_ids(&req_copy.imp);
            adapter_requests.push(RequestData {
                method: "POST".to_string(),
                uri: self.endpoint.clone(),
                body,
                headers,
                imp_ids,
            });
        }

        if adapter_requests.is_empty() {
            return (vec![], vec![BidderError::BadInput("found no valid impressions".to_string())]);
        }

        (adapter_requests, vec![])
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(internal.imp.len());
        if let Some(cur) = &bid_resp.cur {
            result.currency = cur.clone();
        }
        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                match internal.imp.iter().find(|i| i.id == bid.impid) {
                    Some(imp) => {
                        let bid_type = get_bid_type_from_imp(imp);
                        result.bids.push(TypedBid::new(bid, bid_type));
                    }
                    None => {
                        errs.push(BidderError::BadInput(format!("Failed to find impression \"{}\"", bid.impid)));
                    }
                }
            }
        }
        Ok(result)
    }
}
