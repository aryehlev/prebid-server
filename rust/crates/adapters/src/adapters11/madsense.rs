use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids, check_response_status};
use openrtb::BidResponse;
use openrtb_ext::BidType;

pub struct MadsenseAdapter { pub endpoint: String }
impl MadsenseAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

fn get_mtype_bid_type(mtype: u64) -> BidType {
    match mtype {
        2 => BidType::Video,
        4 => BidType::Native,
        _ => BidType::Banner,
    }
}

fn make_single_request(
    endpoint: &str,
    request: &openrtb::BidRequest,
    imps: Vec<openrtb::Imp>,
    company_id: &str,
) -> Result<RequestData, BidderError> {
    let mut req_copy = request.clone();
    req_copy.imp = imps;
    let body = serde_json::to_vec(&req_copy)
        .map_err(|e| BidderError::BadInput(e.to_string()))?;

    let uri = format!("{}?company_id={}", endpoint, company_id);
    let mut headers = HashMap::new();
    headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
    headers.insert("Accept".to_string(), "application/json".to_string());

    Ok(RequestData {
        method: "POST".to_string(),
        uri,
        body,
        headers,
        imp_ids: get_imp_ids(&req_copy.imp),
    })
}

impl Bidder for MadsenseAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut requests = Vec::new();

        // Parse company_id from first imp
        let company_id = if request.test == Some(1) {
            "test".to_string()
        } else {
            request.imp.first()
                .and_then(|imp| imp.ext.as_ref())
                .and_then(|e| e.get("bidder"))
                .and_then(|b| b.get("companyId"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        };

        // Split banner imps (one per request) and collect video imps together
        let mut video_imps: Vec<openrtb::Imp> = Vec::new();

        for imp in &request.imp {
            if imp.banner.is_some() {
                match make_single_request(&self.endpoint, request, vec![imp.clone()], &company_id) {
                    Ok(rd) => requests.push(rd),
                    Err(e) => errs.push(e),
                }
            } else if imp.video.is_some() {
                video_imps.push(imp.clone());
            }
        }

        if !video_imps.is_empty() {
            match make_single_request(&self.endpoint, request, video_imps, &company_id) {
                Ok(rd) => requests.push(rd),
                Err(e) => errs.push(e),
            }
        }

        (requests, errs)
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(5);
        if let Some(cur) = &bid_resp.cur {
            if !cur.is_empty() { result.currency = cur.clone(); }
        }
        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let mtype = bid.ext.as_ref()
                    .and_then(|e| e.get("mtype"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let bid_type = get_mtype_bid_type(mtype);
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        if !errs.is_empty() { return Err(errs); }
        Ok(result)
    }
}
