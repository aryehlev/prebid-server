use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use openrtb_ext::BidType;

pub struct MelozenAdapter { pub endpoint: String }
impl MelozenAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

fn get_media_type_for_bid(bid: &openrtb::Bid) -> Result<BidType, BidderError> {
    let t = bid.ext.as_ref()
        .and_then(|e| e.get("prebid"))
        .and_then(|p| p.get("type"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    match t {
        "banner" => Ok(BidType::Banner),
        "video" => Ok(BidType::Video),
        "native" => Ok(BidType::Native),
        "audio" => Ok(BidType::Audio),
        _ => Err(BidderError::BadServerResponse(format!(
            "Failed to parse bid mediatype for impression \"{}\"", bid.impid
        ))),
    }
}

fn split_impressions_by_media_type(imp: &openrtb::Imp) -> Result<Vec<openrtb::Imp>, BidderError> {
    if imp.banner.is_none() && imp.native.is_none() && imp.video.is_none() {
        return Err(BidderError::BadInput(
            "Invalid MediaType. MeloZen only supports Banner, Video and Native.".to_string()
        ));
    }
    let mut result = Vec::new();
    if imp.banner.is_some() {
        let mut copy = imp.clone();
        copy.video = None;
        copy.native = None;
        result.push(copy);
    }
    if imp.video.is_some() {
        let mut copy = imp.clone();
        copy.banner = None;
        copy.native = None;
        result.push(copy);
    }
    if imp.native.is_some() {
        let mut copy = imp.clone();
        copy.banner = None;
        copy.video = None;
        result.push(copy);
    }
    Ok(result)
}

impl Bidder for MelozenAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _info: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();
        let mut errs = Vec::new();
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        for imp in &request.imp {
            let bidder_val = match imp.ext.as_ref().and_then(|e| e.get("bidder")) {
                Some(v) => v.clone(),
                None => { errs.push(BidderError::BadInput("missing bidder ext".to_string())); continue; }
            };
            let pub_id = bidder_val.get("pubId").and_then(|v| v.as_str()).unwrap_or("");
            let uri = self.endpoint.replace("{{.PublisherID}}", pub_id);

            let split_imps = match split_impressions_by_media_type(imp) {
                Ok(v) => v,
                Err(e) => { errs.push(e); continue; }
            };

            for split_imp in split_imps {
                let mut req_copy = request.clone();
                req_copy.imp = vec![split_imp];
                let imp_ids = req_copy.imp.iter().map(|i| i.id.clone()).collect();
                let body = match serde_json::to_vec(&req_copy) {
                    Ok(b) => b,
                    Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
                };
                requests.push(RequestData { method: "POST".to_string(), uri: uri.clone(), body, headers: headers.clone(), imp_ids });
            }
        }
        (requests, errs)
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::new();
        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                match get_media_type_for_bid(&bid) {
                    Ok(t) => result.bids.push(TypedBid::new(bid, t)),
                    Err(e) => errs.push(e),
                }
            }
        }
        if !errs.is_empty() && result.bids.is_empty() { return Err(errs); }
        Ok(result)
    }
}
