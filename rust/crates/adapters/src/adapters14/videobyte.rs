use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use openrtb_ext::BidType;

pub struct VideobyteAdapter { pub endpoint: String }
impl VideobyteAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

fn get_media_type_for_imp(imp: &openrtb::Imp) -> BidType {
    if imp.banner.is_some() { BidType::Banner } else { BidType::Video }
}

fn get_headers(request: &openrtb::BidRequest) -> HashMap<String, String> {
    let mut h = HashMap::new();
    h.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
    h.insert("Accept".to_string(), "application/json".to_string());
    if let Some(site) = &request.site {
        if let Some(domain) = &site.domain {
            if !domain.is_empty() {
                h.insert("Origin".to_string(), domain.clone());
            }
        }
        if let Some(ref_) = &site.ref_ {
            if !ref_.is_empty() {
                h.insert("Referer".to_string(), ref_.clone());
            }
        }
    }
    h
}

impl Bidder for VideobyteAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();
        let mut errs = Vec::new();
        let original_imps = request.imp.clone();
        let headers = get_headers(request);
        for imp in &original_imps {
            let bidder = imp.ext.as_ref().and_then(|e| e.get("bidder"));
            // ExtImpVideoByte: json:"pubId", json:"placementId", json:"nid"
            let publisher_id = bidder.and_then(|b| b.get("pubId")).and_then(|v| v.as_str()).unwrap_or("");
            let placement_id = bidder.and_then(|b| b.get("placementId")).and_then(|v| v.as_str()).unwrap_or("");
            let network_id = bidder.and_then(|b| b.get("nid")).and_then(|v| v.as_str()).unwrap_or("");

            // Build query params: source=pbs&pid=<publisherId>[&placementId=...][&nid=...]
            let mut params = vec![
                ("source".to_string(), "pbs".to_string()),
                ("pid".to_string(), publisher_id.to_string()),
            ];
            if !placement_id.is_empty() { params.push(("placementId".to_string(), placement_id.to_string())); }
            if !network_id.is_empty() { params.push(("nid".to_string(), network_id.to_string())); }
            let query: String = params.iter().map(|(k, v)| format!("{}={}", k, v)).collect::<Vec<_>>().join("&");
            let uri = format!("{}?{}", self.endpoint, query);

            let mut req_copy = request.clone();
            req_copy.imp = vec![imp.clone()];
            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };
            requests.push(RequestData {
                method: "POST".to_string(),
                uri,
                body,
                headers: headers.clone(),
                imp_ids: vec![imp.id.clone()],
            });
        }
        (requests, errs)
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code == 400 {
            return Err(vec![BidderError::BadInput(format!(
                "Bad user input: HTTP status {}. Run with request.debug = 1 for more info", response.status_code
            ))]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info", response.status_code
            ))]);
        }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|_| vec![BidderError::BadServerResponse("Bad Server Response".to_string())])?;
        let mut result = BidderResponse::with_capacity(1);
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = internal.imp.iter().find(|i| i.id == bid.impid)
                    .map(get_media_type_for_imp).unwrap_or(BidType::Video);
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        Ok(result)
    }
}
