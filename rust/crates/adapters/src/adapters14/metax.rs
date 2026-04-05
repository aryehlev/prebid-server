use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use openrtb_ext::{BidType, ExtBidPrebidVideo};

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

/// Determine bid type from bid.mtype, matching Go getBidType logic.
/// Returns error for unknown/unsupported mtype (matching Go behavior).
fn get_bid_type(mtype: i32) -> Result<BidType, BidderError> {
    match mtype {
        1 => Ok(BidType::Banner),  // MarkupBanner
        2 => Ok(BidType::Video),   // MarkupVideo
        3 => Ok(BidType::Audio),   // MarkupAudio
        4 => Ok(BidType::Native),  // MarkupNative
        _ => Err(BidderError::BadServerResponse(format!("Unsupported MType {}", mtype))),
    }
}

/// Build ExtBidPrebidVideo for a bid, reading category and duration.
/// Go uses bid.Cat[0] and bid.Dur; since Rust openrtb Bid has no `dur` field,
/// we read duration from bid.ext.dur if available.
fn get_bid_video(bid: &openrtb::Bid) -> Option<ExtBidPrebidVideo> {
    let primary_category = bid.cat.as_deref()
        .and_then(|c| c.first())
        .cloned()
        .unwrap_or_default();
    let duration = bid.ext.as_ref()
        .and_then(|e| e.get("dur"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0) as i32;
    Some(ExtBidPrebidVideo { primary_category, duration })
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

        // Additional no-content check (matching Go behavior)
        if bid_resp.seatbid.is_empty() || bid_resp.seatbid[0].bid.is_empty() {
            return Ok(BidderResponse::new());
        }

        let cap = bid_resp.seatbid[0].bid.len();
        let mut result = BidderResponse::with_capacity(cap);
        if let Some(cur) = &bid_resp.cur { if !cur.is_empty() { result.currency = cur.clone(); } }

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let mtype = bid.mtype.unwrap_or(0);
                // Error on unsupported mtype (matching Go: return nil, []error{err})
                let bid_type = match get_bid_type(mtype) {
                    Ok(t) => t,
                    Err(e) => return Err(vec![e]),
                };
                let bid_video = get_bid_video(&bid);
                let mut typed_bid = TypedBid::new(bid, bid_type);
                typed_bid.bid_video = bid_video;
                result.bids.push(typed_bid);
            }
        }
        Ok(result)
    }
}
