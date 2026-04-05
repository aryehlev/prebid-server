use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::{Deserialize, Serialize};

pub struct UnicornAdapter { pub endpoint: String }
impl UnicornAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

#[derive(Debug, Deserialize)]
struct ExtRegs {
    #[serde(rename = "gdpr")]
    gdpr: Option<i32>,
    #[serde(rename = "us_privacy", default)]
    us_privacy: String,
}

impl Bidder for UnicornAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        // COPPA check
        if let Some(regs) = &request.regs {
            if regs.coppa == Some(1) {
                return (vec![], vec![BidderError::BadInput("COPPA is not supported".to_string())]);
            }
            if let Some(ext) = &regs.ext {
                if let Ok(ext_regs) = serde_json::from_value::<ExtRegs>(ext.clone()) {
                    if ext_regs.gdpr == Some(1) {
                        return (vec![], vec![BidderError::BadInput("GDPR is not supported".to_string())]);
                    }
                    if !ext_regs.us_privacy.is_empty() {
                        return (vec![], vec![BidderError::BadInput("CCPA is not supported".to_string())]);
                    }
                }
            }
        }

        // app is required
        if request.app.is_none() {
            return (vec![], vec![BidderError::BadInput("request app is required".to_string())]);
        }

        if request.imp.is_empty() {
            return (vec![], vec![BidderError::BadInput("No impressions".to_string())]);
        }

        // Get accountId from first imp.ext.bidder.accountId
        let account_id = match request.imp[0].ext.as_ref()
            .and_then(|e| e.get("bidder"))
            .and_then(|b| b.get("accountId"))
            .and_then(|v| v.as_i64())
        {
            Some(id) => id,
            None => {
                return (vec![], vec![BidderError::BadInput("accountId field is required".to_string())]);
            }
        };

        let mut req_copy = request.clone();

        // Modify imps
        for (i, imp) in req_copy.imp.iter_mut().enumerate() {
            let placement_id = imp.ext.as_ref()
                .and_then(|e| e.get("bidder"))
                .and_then(|b| b.get("placementId"))
                .and_then(|v| v.as_str())
                .unwrap_or("").to_string();

            let final_placement_id = if placement_id.is_empty() {
                // Try storedrequest id
                let stored_id = imp.ext.as_ref()
                    .and_then(|e| e.get("prebid"))
                    .and_then(|p| p.get("storedrequest"))
                    .and_then(|s| s.get("id"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                match stored_id {
                    Some(id) => id,
                    None => {
                        return (vec![], vec![BidderError::BadInput(
                            format!("Error get StoredRequestImpID from imp[{}]: stored request id not found", i)
                        )]);
                    }
                }
            } else {
                placement_id
            };

            imp.tagid = Some(final_placement_id.clone());
            imp.secure = Some(1);

            // Rebuild imp ext keeping context and updating bidder.placementId
            if let Some(ext) = &imp.ext {
                let mut ext_copy = ext.clone();
                if let Some(obj) = ext_copy.as_object_mut() {
                    if let Some(bidder) = obj.get_mut("bidder") {
                        if let Some(b) = bidder.as_object_mut() {
                            b.insert("placementId".to_string(), serde_json::Value::String(final_placement_id));
                        }
                    }
                }
                imp.ext = Some(ext_copy);
            }
        }

        // Modify app
        if let Some(app) = &req_copy.app {
            let mut app_copy = app.clone();
            // Set app.id from bidder.mediaId if available
            if let Some(media_id) = request.imp[0].ext.as_ref()
                .and_then(|e| e.get("bidder"))
                .and_then(|b| b.get("mediaId"))
                .and_then(|v| v.as_str())
            {
                app_copy.id = Some(media_id.to_string());
            }
            // Set publisher.id from bidder.publisherId if available
            if let Some(pub_id) = request.imp[0].ext.as_ref()
                .and_then(|e| e.get("bidder"))
                .and_then(|b| b.get("publisherId"))
                .and_then(|v| v.as_str())
            {
                let mut publisher = app_copy.publisher.clone().unwrap_or_default();
                publisher.id = Some(pub_id.to_string());
                app_copy.publisher = Some(publisher);
            }
            req_copy.app = Some(app_copy);
        }

        // Set source ext
        let source_ext = serde_json::json!({"stype": "prebid_server_uncn", "bidder": "unicorn"});
        let mut source = req_copy.source.clone().unwrap_or_default();
        source.ext = Some(source_ext);
        req_copy.source = Some(source);

        // Set request ext with accountId
        let mut ext_obj = req_copy.ext.clone().and_then(|e| {
            if let serde_json::Value::Object(m) = e { Some(m) } else { None }
        }).unwrap_or_default();
        ext_obj.insert("accountId".to_string(), serde_json::Value::Number(account_id.into()));
        req_copy.ext = Some(serde_json::Value::Object(ext_obj));

        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
        headers.insert("X-Openrtb-Version".to_string(), "2.5".to_string());
        if let Some(device) = &request.device {
            if let Some(ua) = &device.ua { if !ua.is_empty() { headers.insert("User-Agent".to_string(), ua.clone()); } }
            if let Some(ipv6) = &device.ipv6 { if !ipv6.is_empty() { headers.insert("X-Forwarded-For".to_string(), ipv6.clone()); } }
            if let Some(ip) = &device.ip { if !ip.is_empty() { headers.insert("X-Forwarded-For".to_string(), ip.clone()); } }
        }

        (vec![RequestData {
            method: "POST".to_string(),
            uri: self.endpoint.clone(),
            body,
            headers,
            imp_ids: get_imp_ids(&request.imp),
        }], vec![])
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code == 400 {
            return Err(vec![BidderError::BadInput("Unexpected http status code: 400".to_string())]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(
                format!("Unexpected http status code: {}", response.status_code)
            )]);
        }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(internal.imp.len());
        result.currency = bid_resp.cur.clone().unwrap_or_default();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = internal.imp.iter().find(|i| i.id == bid.impid)
                    .and_then(|imp| if imp.banner.is_some() { Some(BidType::Banner) } else { None })
                    .unwrap_or(BidType::Banner);
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        Ok(result)
    }
}
