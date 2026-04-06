use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids, check_response_status};
use openrtb::BidResponse;
use openrtb_ext::BidType;

pub struct ClydoAdapter { pub endpoint: String }
impl ClydoAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

impl Bidder for ClydoAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let imp_ids = get_imp_ids(&request.imp);
        if imp_ids.is_empty() {
            return (vec![], vec![BidderError::BadInput("Failed to get imp ids".to_string())]);
        }

        // Require device
        if request.device.is_none() {
            return (vec![], vec![BidderError::BadInput("Failed to get device headers".to_string())]);
        }
        let device = request.device.as_ref().unwrap();

        let mut base_headers = HashMap::new();
        base_headers.insert("Content-Type".to_string(), "application/json; charset=utf-8".to_string());
        base_headers.insert("Accept".to_string(), "application/json".to_string());
        base_headers.insert("X-OpenRTB-Version".to_string(), "2.5".to_string());
        if let Some(ipv6) = &device.ipv6 { if !ipv6.is_empty() { base_headers.insert("X-Forwarded-For".to_string(), ipv6.clone()); } }
        if let Some(ip) = &device.ip { if !ip.is_empty() { base_headers.insert("X-Forwarded-For".to_string(), ip.clone()); } }
        if let Some(ua) = &device.ua { if !ua.is_empty() { base_headers.insert("User-Agent".to_string(), ua.clone()); } }

        let mut requests = Vec::new();
        let mut errs = Vec::new();

        for imp in &request.imp {
            let bidder = imp.ext.as_ref()
                .and_then(|e| e.get("bidder"))
                .cloned()
                .unwrap_or(serde_json::Value::Null);

            let partner_id = bidder.get("partnerId").and_then(|v| v.as_str()).unwrap_or("");
            if partner_id.is_empty() {
                errs.push(BidderError::BadInput("invalid partnerId".to_string()));
                continue;
            }
            let region = bidder.get("region").and_then(|v| v.as_str()).unwrap_or("us");

            let url = self.endpoint
                .replace("{{.PartnerId}}", partner_id)
                .replace("{{.Region}}", region);

            let mut req_copy = request.clone();
            req_copy.imp = vec![imp.clone()];

            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };

            requests.push(RequestData {
                method: "POST".to_string(),
                uri: url,
                body,
                headers: base_headers.clone(),
                imp_ids: imp_ids.clone(),
            });
        }

        (requests, errs)
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = check_response_status(response.status_code) { return Err(vec![e]); }

        let bid_resp: BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(internal.imp.len());
        if let Some(cur) = &bid_resp.cur {
            if !cur.is_empty() { result.currency = cur.clone(); }
        }

        // Build bid type map from imps; reject duplicate imp IDs and imps with no known media type
        let mut bid_type_map: HashMap<String, BidType> = HashMap::new();
        for imp in &internal.imp {
            if bid_type_map.contains_key(&imp.id) {
                return Err(vec![BidderError::BadInput("Duplicate impression ID found".to_string())]);
            }
            let t = if imp.audio.is_some() { BidType::Audio }
                else if imp.video.is_some() { BidType::Video }
                else if imp.native.is_some() { BidType::Native }
                else if imp.banner.is_some() { BidType::Banner }
                else {
                    return Err(vec![BidderError::BadInput("Failed to get media type".to_string())]);
                };
            bid_type_map.insert(imp.id.clone(), t);
        }

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = bid_type_map.get(&bid.impid)
                    .cloned()
                    .unwrap_or(BidType::Banner);
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        Ok(result)
    }
}
