use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub struct AllianceGravityAdapter {
    pub endpoint: String,
}

impl AllianceGravityAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

#[derive(Deserialize)]
struct ExtImpBidder {
    bidder: Value,
    #[serde(default)]
    prebid: Option<Value>,
}

#[derive(Deserialize)]
struct ExtImpAllianceGravity {
    #[serde(rename = "srid", default)]
    sr_id: String,
}

#[derive(Serialize)]
struct ExtStoredRequest {
    id: String,
}

#[derive(Serialize)]
struct ExtImpPrebid {
    #[serde(rename = "storedrequest")]
    stored_request: ExtStoredRequest,
}

#[derive(Serialize)]
struct GeneratedExt {
    bidder: Value,
    prebid: ExtImpPrebid,
}

fn get_media_type_for_bid(bid: &openrtb::Bid) -> Result<BidType, BidderError> {
    // Parse bid.ext.prebid.type
    if let Some(ext) = &bid.ext {
        if let Ok(obj) = serde_json::from_value::<serde_json::Map<String, Value>>(ext.clone()) {
            if let Some(prebid) = obj.get("prebid") {
                if let Some(bid_type) = prebid.get("type").and_then(|v| v.as_str()) {
                    return match bid_type {
                        "banner" => Ok(BidType::Banner),
                        "video" => Ok(BidType::Video),
                        "native" => Ok(BidType::Native),
                        "audio" => Ok(BidType::Audio),
                        other => Err(BidderError::BadServerResponse(format!(
                            "Failed to parse impression \"{}\" mediatype", bid.impid
                        ))),
                    };
                }
            }
        }
    }
    Err(BidderError::BadServerResponse(format!(
        "Failed to parse impression \"{}\" mediatype", bid.impid
    )))
}

impl Bidder for AllianceGravityAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        // Generate imps with modified ext setting prebid.storedRequest
        let mut generated_imps = Vec::with_capacity(request.imp.len());
        for imp in &request.imp {
            let ext_val = match &imp.ext {
                Some(v) => v.clone(),
                None => return (vec![], vec![BidderError::BadInput("missing imp ext".to_string())]),
            };

            let bidder_ext: ExtImpBidder = match serde_json::from_value(ext_val) {
                Ok(v) => v,
                Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
            };

            let ag_ext: ExtImpAllianceGravity = match serde_json::from_value(bidder_ext.bidder) {
                Ok(v) => v,
                Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
            };

            let new_ext = GeneratedExt {
                bidder: serde_json::json!({}),
                prebid: ExtImpPrebid {
                    stored_request: ExtStoredRequest { id: ag_ext.sr_id },
                },
            };

            let new_ext_val = match serde_json::to_value(&new_ext) {
                Ok(v) => v,
                Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
            };

            let mut imp_copy = imp.clone();
            imp_copy.ext = Some(new_ext_val);
            generated_imps.push(imp_copy);
        }

        let mut req_copy = request.clone();
        req_copy.imp = generated_imps;

        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        if let Some(device) = &request.device {
            if let Some(ua) = &device.ua {
                if !ua.is_empty() {
                    headers.insert("User-Agent".to_string(), ua.clone());
                }
            }
            if let Some(ip) = &device.ip {
                if !ip.is_empty() {
                    headers.insert("X-Forwarded-For".to_string(), ip.clone());
                }
            } else if let Some(ipv6) = &device.ipv6 {
                if !ipv6.is_empty() {
                    headers.insert("X-Forwarded-For".to_string(), ipv6.clone());
                }
            }
        }
        if let Some(site) = &request.site {
            if let Some(page) = &site.page {
                if !page.is_empty() {
                    headers.insert("Referer".to_string(), page.clone());
                }
            }
        }
        if let Some(user) = &request.user {
            if let Some(buyeruid) = &user.buyeruid {
                if !buyeruid.is_empty() {
                    headers.insert("Cookie".to_string(), format!("uids={}", buyeruid));
                }
            }
        }

        (vec![RequestData {
            method: "POST".to_string(),
            uri: self.endpoint.clone(),
            body,
            headers,
            imp_ids: get_imp_ids(&req_copy.imp),
        }], vec![])
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code == 400 {
            return Err(vec![BidderError::BadInput(
                "Unexpected status code: 400. Bad request from publisher. Run with request.debug = 1 for more info.".to_string()
            )]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info.", response.status_code
            ))]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut bids: Vec<TypedBid> = Vec::new();
        let mut errors: Vec<BidderError> = Vec::new();

        for sb in &bid_resp.seatbid {
            for bid in &sb.bid {
                match get_media_type_for_bid(bid) {
                    Ok(bid_type) => bids.push(TypedBid::new(bid.clone(), bid_type)),
                    Err(e) => errors.push(e),
                }
            }
        }

        if bids.is_empty() {
            return Ok(BidderResponse::new());
        }

        let mut result = BidderResponse::with_capacity(bids.len());
        if let Some(cur) = &bid_resp.cur {
            if !cur.is_empty() {
                result.currency = cur.clone();
            }
        }
        result.bids = bids;

        Ok(result)
    }
}
