use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct YieldoneAdapter { pub endpoint: String }
impl YieldoneAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

fn preprocess_imp(imp: &mut openrtb::Imp) -> Result<(), BidderError> {
    // validate bidder ext exists
    let _bidder_val = imp.ext.as_ref().and_then(|e| e.get("bidder")).ok_or_else(|| {
        BidderError::BadInput("missing bidder ext".to_string())
    })?;

    // Fix banner size from format if w/h not set
    if let Some(banner) = imp.banner.as_mut() {
        if banner.w.is_none() && banner.h.is_none() {
            if let Some(formats) = &banner.format {
                if !formats.is_empty() {
                    let first = formats[0].clone();
                    banner.w = first.w;
                    banner.h = first.h;
                }
            }
        }
    }
    Ok(())
}

fn get_media_type_for_imp(imp_id: &str, imps: &[openrtb::Imp]) -> Result<BidType, BidderError> {
    for imp in imps {
        if imp.id == imp_id {
            if imp.banner.is_some() { return Ok(BidType::Banner); }
            if imp.video.is_some() { return Ok(BidType::Video); }
            return Err(BidderError::BadServerResponse(format!("Unknown impression type for ID: \"{}\"", imp_id)));
        }
    }
    Err(BidderError::BadServerResponse(format!("Unknown impression type for ID: \"{}\"", imp_id)))
}

impl Bidder for YieldoneAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut valid_imps = Vec::new();
        let mut req_copy = request.clone();

        for imp in &mut req_copy.imp {
            match preprocess_imp(imp) {
                Ok(()) => valid_imps.push(imp.clone()),
                Err(e) => errs.push(e),
            }
        }
        req_copy.imp = valid_imps;

        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => {
                errs.push(BidderError::BadInput(e.to_string()));
                return (vec![], errs);
            }
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());

        (vec![RequestData {
            method: "POST".to_string(),
            uri: self.endpoint.clone(),
            body,
            headers,
            imp_ids: get_imp_ids(&req_copy.imp),
        }], errs)
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code == 400 {
            return Err(vec![BidderError::BadInput(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info", response.status_code
            ))]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info", response.status_code
            ))]);
        }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(1);
        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                match get_media_type_for_imp(&bid.impid, &internal.imp) {
                    Ok(t) => result.bids.push(TypedBid::new(bid, t)),
                    Err(e) => errs.push(e),
                }
            }
        }
        if let Some(cur) = &bid_resp.cur {
            if !cur.is_empty() { result.currency = cur.clone(); }
        }
        if !errs.is_empty() && result.bids.is_empty() { return Err(errs); }
        Ok(result)
    }
}
