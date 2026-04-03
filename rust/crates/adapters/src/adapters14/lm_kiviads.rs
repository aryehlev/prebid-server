use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct LmKiviadsAdapter { pub endpoint: String }
impl LmKiviadsAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

impl Bidder for LmKiviadsAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();
        let mut errs = Vec::new();
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        for imp in &request.imp {
            let bidder = imp.ext.as_ref().and_then(|e| e.get("bidder"));
            let env = bidder.and_then(|b| b.get("env")).and_then(|v| v.as_str()).unwrap_or("");
            let pid = bidder.and_then(|b| b.get("pid")).and_then(|v| v.as_str()).unwrap_or("");
            let uri = self.endpoint
                .replace("{{.Host}}", env)
                .replace("{{.SourceId}}", pid);

            let mut req_copy = request.clone();
            req_copy.imp = vec![imp.clone()];
            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };
            requests.push(RequestData { method: "POST".to_string(), uri, body, headers: headers.clone(), imp_ids: vec![imp.id.clone()] });
        }
        (requests, errs)
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code == 503 {
            return Err(vec![BidderError::BadInput("Bidder LmKiviads is unavailable. Please contact the bidder support.".to_string())]);
        }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        if bid_resp.seatbid.is_empty() {
            return Err(vec![BidderError::BadServerResponse("Array SeatBid cannot be empty".to_string())]);
        }
        let mut result = BidderResponse::with_capacity(bid_resp.seatbid.len());
        let mut errs = Vec::new();
        for (seat_idx, sb) in bid_resp.seatbid.into_iter().enumerate() {
            for (bid_idx, bid) in sb.bid.into_iter().enumerate() {
                let type_str = bid.ext.as_ref()
                    .and_then(|e| e.get("prebid"))
                    .and_then(|p| p.get("type"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let bid_type = match type_str {
                    "banner" => BidType::Banner,
                    "video" => BidType::Video,
                    "native" => BidType::Native,
                    "audio" => BidType::Audio,
                    other => {
                        errs.push(BidderError::BadServerResponse(format!(
                            "Bid[{}].Ext.Prebid.Type expects one of the following values: 'banner', 'native', 'video', 'audio', got '{}'",
                            bid_idx, other
                        )));
                        continue;
                    }
                };
                let _ = seat_idx; // suppress warning
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        if !errs.is_empty() && result.bids.is_empty() { return Err(errs); }
        Ok(result)
    }
}
