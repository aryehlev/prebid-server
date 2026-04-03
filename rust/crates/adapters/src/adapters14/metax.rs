use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_bid_type_from_mtype};
use openrtb_ext::ExtBidPrebidVideo;

pub struct MetaxAdapter { pub endpoint: String }
impl MetaxAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

fn assign_banner_size(banner: &mut openrtb::Banner) {
    if banner.w.is_none() || banner.h.is_none() {
        if let Some(fmt) = banner.format.as_deref().and_then(|f| f.first()) {
            banner.w = Some(fmt.w.unwrap_or(0));
            banner.h = Some(fmt.h.unwrap_or(0));
        }
    }
}

impl Bidder for MetaxAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();
        let mut errs = Vec::new();
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        for imp in &request.imp {
            let bidder_val = match imp.ext.as_ref().and_then(|e| e.get("bidder")) {
                Some(v) => v.clone(),
                None => { errs.push(BidderError::BadInput("Wrong MetaX bidder ext".to_string())); continue; }
            };
            let publisher_id = bidder_val.get("publisherId").and_then(|v| v.as_i64()).unwrap_or(0);
            let adunit = bidder_val.get("adunit").and_then(|v| v.as_i64()).unwrap_or(0);
            let uri = self.endpoint
                .replace("{{.PublisherID}}", &publisher_id.to_string())
                .replace("{{.AdUnit}}", &adunit.to_string());

            let mut imp_copy = imp.clone();
            if let Some(banner) = imp_copy.banner.as_mut() {
                assign_banner_size(banner);
            }

            let mut req_copy = request.clone();
            req_copy.imp = vec![imp_copy];
            let imp_ids = req_copy.imp.iter().map(|i| i.id.clone()).collect();
            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); return (vec![], errs); }
            };
            requests.push(RequestData { method: "POST".to_string(), uri, body, headers: headers.clone(), imp_ids });
        }
        (requests, errs)
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        if bid_resp.seatbid.is_empty() || bid_resp.seatbid[0].bid.is_empty() {
            return Ok(BidderResponse::new());
        }
        let cap = bid_resp.seatbid[0].bid.len();
        let mut result = BidderResponse::with_capacity(cap);
        if let Some(cur) = &bid_resp.cur { if !cur.is_empty() { result.currency = cur.clone(); } }
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let mtype = bid.mtype.unwrap_or(0);
                if mtype == 0 {
                    return Err(vec![BidderError::BadServerResponse(format!("Unsupported MType {}", mtype))]);
                }
                let bid_type = get_bid_type_from_mtype(mtype);
                let primary_category = bid.cat.as_deref().and_then(|c| c.first()).cloned().unwrap_or_default();
                let duration = bid.ext.as_ref()
                    .and_then(|e| e.get("dur"))
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0) as i32;
                let bid_video = if duration > 0 || !primary_category.is_empty() {
                    Some(ExtBidPrebidVideo { primary_category, duration })
                } else {
                    None
                };
                let mut typed_bid = TypedBid::new(bid, bid_type);
                typed_bid.bid_video = bid_video;
                result.bids.push(typed_bid);
            }
        }
        Ok(result)
    }
}
