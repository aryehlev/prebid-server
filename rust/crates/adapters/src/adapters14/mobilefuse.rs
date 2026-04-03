use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use openrtb_ext::BidType;

pub struct MobilefuseAdapter { pub endpoint: String }
impl MobilefuseAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

fn get_bid_type(bid: &openrtb::Bid) -> BidType {
    let media_type = bid.ext.as_ref()
        .and_then(|e| e.get("mf"))
        .and_then(|mf| mf.get("media_type"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    match media_type {
        "video" => BidType::Video,
        "native" => BidType::Native,
        _ => BidType::Banner,
    }
}

fn get_first_placement_id(request: &openrtb::BidRequest) -> Option<i64> {
    for imp in &request.imp {
        if let Some(pid) = imp.ext.as_ref()
            .and_then(|e| e.get("bidder"))
            .and_then(|b| b.get("placementId"))
            .and_then(|v| v.as_i64())
        {
            return Some(pid);
        }
    }
    None
}

impl Bidder for MobilefuseAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();

        let placement_id = get_first_placement_id(request);

        // Filter valid imps (banner, video, or native only)
        let mut valid_imps: Vec<openrtb::Imp> = Vec::new();
        for imp in &request.imp {
            if imp.banner.is_none() && imp.video.is_none() && imp.native.is_none() {
                continue;
            }
            let mut imp_copy = imp.clone();
            // Set tagid from placement id
            if let Some(pid) = placement_id {
                imp_copy.tagid = Some(pid.to_string());
            }
            // Strip ext to only skadn if present
            let skadn = imp.ext.as_ref()
                .and_then(|e| e.get("skadn"))
                .cloned();
            imp_copy.ext = if let Some(sk) = skadn {
                let mut m = serde_json::Map::new();
                m.insert("skadn".to_string(), sk);
                Some(serde_json::Value::Object(m))
            } else {
                None
            };
            valid_imps.push(imp_copy);
        }

        if valid_imps.is_empty() {
            errs.push(BidderError::BadInput("No valid imps".to_string()));
            return (vec![], errs);
        }

        let mut req_copy = request.clone();
        req_copy.imp = valid_imps;
        let imp_ids = req_copy.imp.iter().map(|i| i.id.clone()).collect();
        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
        (vec![RequestData { method: "POST".to_string(), uri: self.endpoint.clone(), body, headers, imp_ids }], errs)
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code == 400 {
            return Err(vec![BidderError::BadInput(format!("Unexpected status code: {}.", response.status_code))]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!("Unexpected status code: {}.", response.status_code))]);
        }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(1);
        for sb in bid_resp.seatbid {
            for mut bid in sb.bid {
                let bid_type = get_bid_type(&bid);
                bid.ext = None;
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        Ok(result)
    }
}
