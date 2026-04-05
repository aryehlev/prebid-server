use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_bid_type_from_imp};

pub struct FlatadsAdapter { pub endpoint: String }
impl FlatadsAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

impl Bidder for FlatadsAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();
        let mut errs = Vec::new();

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
        if let Some(device) = &request.device {
            if let Some(ua) = &device.ua { if !ua.is_empty() { headers.insert("User-Agent".to_string(), ua.clone()); } }
            if let Some(ipv6) = &device.ipv6 { if !ipv6.is_empty() { headers.insert("X-Forwarded-For".to_string(), ipv6.clone()); } }
            if let Some(ip) = &device.ip { if !ip.is_empty() { headers.insert("X-Forwarded-For".to_string(), ip.clone()); } }
        }

        for imp in &request.imp {
            let bidder = imp.ext.as_ref()
                .and_then(|e| e.get("bidder"))
                .cloned()
                .unwrap_or(serde_json::Value::Null);

            let token = bidder.get("token").and_then(|v| v.as_str()).unwrap_or("");
            let publisher_id = bidder.get("publisherId").and_then(|v| v.as_str()).unwrap_or("");

            // Build URL from template: replace {{.TokenID}} and {{.PublisherID}}
            let url = self.endpoint
                .replace("{{.TokenID}}", token)
                .replace("{{.PublisherID}}", publisher_id);

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
                headers: headers.clone(),
                imp_ids: vec![imp.id.clone()],
            });
        }

        (requests, errs)
    }

    fn make_bids(&self, request: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(request.imp.len());
        if let Some(cur) = &bid_resp.cur {
            if !cur.is_empty() {
                result.currency = cur.clone();
            }
        }

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                // Get bid type from imp
                match request.imp.iter().find(|i| i.id == bid.impid) {
                    Some(imp) => {
                        let bid_type = if imp.banner.is_some() {
                            openrtb_ext::BidType::Banner
                        } else if imp.video.is_some() {
                            openrtb_ext::BidType::Video
                        } else if imp.native.is_some() {
                            openrtb_ext::BidType::Native
                        } else {
                            // Skip bids with unknown type (matches Go behavior returning error)
                            continue;
                        };
                        result.bids.push(TypedBid::new(bid, bid_type));
                    }
                    None => {
                        // Skip bids for imps not in request (matches Go behavior)
                        continue;
                    }
                }
            }
        }

        Ok(result)
    }
}
