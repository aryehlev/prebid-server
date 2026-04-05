use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_bid_type_from_imp, get_imp_ids};

pub struct ImpactifyAdapter { pub endpoint: String }
impl ImpactifyAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

impl Bidder for ImpactifyAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::BadInput("No valid impressions in the bid request".to_string())]);
        }

        let mut req = request.clone();

        // Re-map imp.ext from {bidder: {...}} to {impactify: {...}}
        for (i, imp) in req.imp.iter_mut().enumerate() {
            let bidder_val = imp.ext.as_ref()
                .and_then(|e| e.get("bidder"))
                .cloned()
                .unwrap_or(serde_json::Value::Null);

            if bidder_val.is_null() {
                return (vec![], vec![BidderError::BadInput(
                    format!("Unable to decode the imp ext : \"{}\"", request.imp[i].id)
                )]);
            }

            imp.ext = Some(serde_json::json!({ "impactify": bidder_val }));
        }

        // Set currency to USD
        req.cur = Some(vec!["USD".to_string()]);

        let body = match serde_json::to_vec(&req) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
        headers.insert("x-openrtb-version".to_string(), "2.5".to_string());
        if let Some(device) = &request.device {
            if let Some(ua) = &device.ua { if !ua.is_empty() { headers.insert("User-Agent".to_string(), ua.clone()); } }
            if let Some(ip) = &device.ip { if !ip.is_empty() { headers.insert("X-Forwarded-For".to_string(), ip.clone()); }
            } else if let Some(ipv6) = &device.ipv6 { if !ipv6.is_empty() { headers.insert("X-Forwarded-For".to_string(), ipv6.clone()); } }
        }
        if let Some(site) = &request.site {
            if let Some(page) = &site.page { if !page.is_empty() { headers.insert("Referer".to_string(), page.clone()); } }
        }
        if let Some(user) = &request.user {
            if let Some(buyer_uid) = &user.buyeruid {
                if !buyer_uid.is_empty() { headers.insert("Cookie".to_string(), format!("uids={}", buyer_uid)); }
            }
        }

        (vec![RequestData {
            method: "POST".to_string(),
            uri: self.endpoint.clone(),
            body,
            headers,
            imp_ids: get_imp_ids(&request.imp),
        }], vec![])
    }

    fn make_bids(&self, request: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code == 400 {
            return Err(vec![BidderError::BadInput("Invalid request.".to_string())]);
        }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|_| vec![BidderError::BadServerResponse("Bad server body response".to_string())])?;

        if bid_resp.seatbid.is_empty() {
            return Ok(BidderResponse::new());
        }

        let sb = &bid_resp.seatbid[0];
        let mut result = BidderResponse::with_capacity(sb.bid.len());
        if let Some(cur) = &bid_resp.cur {
            result.currency = cur.clone();
        }

        for bid in &sb.bid {
            if bid.price <= 0.0 {
                continue;
            }
            match request.imp.iter().find(|i| i.id == bid.impid) {
                Some(imp) => {
                    let bid_type = get_bid_type_from_imp(imp);
                    result.bids.push(TypedBid::new(bid.clone(), bid_type));
                }
                None => {
                    return Err(vec![BidderError::BadInput(
                        format!("Failed to find a supported media type impression \"{}\"", bid.impid)
                    )]);
                }
            }
        }
        Ok(result)
    }
}
