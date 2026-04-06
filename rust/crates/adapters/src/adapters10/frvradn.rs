use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids, check_response_status};
use openrtb::BidResponse;
use openrtb_ext::BidType;

pub struct FrvradnAdapter { pub endpoint: String }
impl FrvradnAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

/// Get bid type from bid.ext.prebid.type
fn get_bid_type_from_bid_ext(bid: &openrtb::Bid) -> Result<BidType, BidderError> {
    let type_str = bid.ext.as_ref()
        .and_then(|e| e.get("prebid"))
        .and_then(|p| p.get("type"))
        .and_then(|v| v.as_str());
    match type_str {
        Some("banner") => Ok(BidType::Banner),
        Some("video") => Ok(BidType::Video),
        Some("native") => Ok(BidType::Native),
        _ => Err(BidderError::BadServerResponse(
            format!("imp {} with unknown media type", bid.impid)
        )),
    }
}

impl Bidder for FrvradnAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _info: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();
        let mut errs = Vec::new();

        for imp in &request.imp {
            let bidder = imp.ext.as_ref()
                .and_then(|e| e.get("bidder"))
                .cloned()
                .unwrap_or(serde_json::Value::Null);

            let publisher_id = bidder.get("publisher_id").and_then(|v| v.as_str()).unwrap_or("");
            let ad_unit_id = bidder.get("ad_unit_id").and_then(|v| v.as_str()).unwrap_or("");

            if publisher_id.is_empty() || ad_unit_id.is_empty() {
                errs.push(BidderError::BadInput("publisher_id and ad_unit_id are required".to_string()));
                continue;
            }

            // Replace imp.ext with just the bidder fields (publisher_id, ad_unit_id)
            let new_ext = serde_json::json!({
                "publisher_id": publisher_id,
                "ad_unit_id": ad_unit_id,
            });

            let mut imp_copy = imp.clone();
            imp_copy.ext = Some(new_ext);

            // NOTE: Go version converts imp.bid_floor currency to USD here via
            // requestInfo.ConvertCurrency(). That functionality is not available in
            // ExtraRequestInfo in this Rust port, so floor currency conversion is skipped.

            let mut req_copy = request.clone();
            req_copy.imp = vec![imp_copy];

            let imp_ids = get_imp_ids(&req_copy.imp);
            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };

            requests.push(RequestData {
                method: "POST".to_string(),
                uri: self.endpoint.clone(),
                body,
                headers: HashMap::new(),
                imp_ids,
            });
        }

        (requests, errs)
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = check_response_status(response.status_code) { return Err(vec![e]); }
        if response.body.is_empty() { return Ok(BidderResponse::new()); }

        let bid_resp: BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(5);
        if let Some(cur) = &bid_resp.cur {
            if !cur.is_empty() { result.currency = cur.clone(); }
        }
        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                match get_bid_type_from_bid_ext(&bid) {
                    Ok(t) => result.bids.push(TypedBid::new(bid, t)),
                    Err(e) => errs.push(e),
                }
            }
        }
        // Match Go: return partial bids alongside errors; only return Err when no bids succeeded
        if !errs.is_empty() && result.bids.is_empty() { return Err(errs); }
        Ok(result)
    }
}
