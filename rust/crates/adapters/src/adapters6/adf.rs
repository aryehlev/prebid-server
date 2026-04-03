use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde_json::Value;

pub struct AdfAdapter {
    pub endpoint: String,
}

impl AdfAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

fn get_bid_type_from_bid(bid: &openrtb::Bid) -> Result<BidType, BidderError> {
    // Try bid.ext.prebid.type first
    if let Some(ext) = &bid.ext {
        if let Some(prebid) = ext.get("prebid") {
            if let Some(t) = prebid.get("type").and_then(|v| v.as_str()) {
                return match t {
                    "banner" => Ok(BidType::Banner),
                    "video" => Ok(BidType::Video),
                    "native" => Ok(BidType::Native),
                    "audio" => Ok(BidType::Audio),
                    _ => Err(BidderError::BadServerResponse(
                        format!("Failed to parse impression \"{}\" mediatype", bid.impid)
                    )),
                };
            }
        }
    }
    Err(BidderError::BadServerResponse(
        format!("Failed to parse impression \"{}\" mediatype", bid.impid)
    ))
}

impl Bidder for AdfAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errors = Vec::new();
        let mut valid_imps: Vec<openrtb::Imp> = Vec::new();
        let mut price_type = String::new();

        for imp in &request.imp {
            let bidder_ext = match imp.ext.as_ref().and_then(|e| e.get("bidder")) {
                Some(b) => b.clone(),
                None => {
                    errors.push(BidderError::BadInput(format!(
                        "imp {} missing ext.bidder", imp.id
                    )));
                    continue;
                }
            };

            // Extract masterTagId (may be number or string)
            let master_tag_id = match bidder_ext.get("mid")
                .or_else(|| bidder_ext.get("masterTagId"))
            {
                Some(v) => match v {
                    Value::Number(n) => n.to_string(),
                    Value::String(s) => s.clone(),
                    _ => {
                        errors.push(BidderError::BadInput(format!(
                            "imp {} invalid masterTagId", imp.id
                        )));
                        continue;
                    }
                },
                None => {
                    errors.push(BidderError::BadInput(format!(
                        "imp {} missing masterTagId", imp.id
                    )));
                    continue;
                }
            };

            // Extract optional priceType
            if let Some(pt) = bidder_ext.get("priceType").and_then(|v| v.as_str()) {
                if !pt.is_empty() && price_type.is_empty() {
                    price_type = pt.to_string();
                }
            }

            let mut imp_copy = imp.clone();
            imp_copy.tagid = Some(master_tag_id);
            valid_imps.push(imp_copy);
        }

        if valid_imps.is_empty() {
            return (vec![], errors);
        }

        let mut req = request.clone();
        req.imp = valid_imps;

        // If priceType is set, inject it into request.ext as { "pt": priceType }
        if !price_type.is_empty() {
            let mut ext_map: serde_json::Map<String, Value> = match &req.ext {
                Some(Value::Object(m)) => m.clone(),
                _ => serde_json::Map::new(),
            };
            ext_map.insert("pt".to_string(), Value::String(price_type));
            req.ext = Some(Value::Object(ext_map));
        }

        let body = match serde_json::to_vec(&req) {
            Ok(b) => b,
            Err(e) => {
                errors.push(BidderError::BadInput(e.to_string()));
                return (vec![], errors);
            }
        };

        let imp_ids = get_imp_ids(&req.imp);
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        (vec![RequestData {
            method: "POST".to_string(),
            uri: self.endpoint.clone(),
            body,
            headers,
            imp_ids,
        }], errors)
    }

    fn make_bids(
        &self,
        _internal: &openrtb::BidRequest,
        _external: &RequestData,
        response: &ResponseData,
    ) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 {
            return Ok(BidderResponse::new());
        }
        if response.status_code == 400 {
            return Err(vec![BidderError::BadInput(
                "Unexpected status code: 400. Bad request from publisher.".to_string()
            )]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected status code: {}.", response.status_code
            ))]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(5);
        if let Some(cur) = &bid_resp.cur {
            if !cur.is_empty() {
                result.currency = cur.clone();
            }
        }

        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                match get_bid_type_from_bid(&bid) {
                    Ok(bid_type) => result.bids.push(TypedBid::new(bid, bid_type)),
                    Err(e) => errs.push(e),
                }
            }
        }

        if result.bids.is_empty() && !errs.is_empty() {
            return Err(errs);
        }

        Ok(result)
    }
}
