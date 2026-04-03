use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct DianomiAdapter { pub endpoint: String }
impl DianomiAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

fn get_bid_type_from_bid_ext(bid: &openrtb::Bid) -> Result<BidType, BidderError> {
    // dianomi returns bid type in bid.ext.prebid.type
    let type_str = bid.ext.as_ref()
        .and_then(|e| e.get("prebid"))
        .and_then(|p| p.get("type"))
        .and_then(|v| v.as_str());
    match type_str {
        Some("banner") => Ok(BidType::Banner),
        Some("video") => Ok(BidType::Video),
        Some("native") => Ok(BidType::Native),
        Some("audio") => Ok(BidType::Audio),
        _ => Err(BidderError::BadServerResponse(
            format!("Failed to parse impression \"{}\" mediatype", bid.impid)
        )),
    }
}

impl Bidder for DianomiAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut req = request.clone();
        let mut errs = Vec::new();
        let mut price_type = String::new();
        let mut valid_imps = Vec::new();

        for mut imp in req.imp.drain(..) {
            let bidder = imp.ext.as_ref()
                .and_then(|e| e.get("bidder"))
                .cloned()
                .unwrap_or(serde_json::Value::Null);

            // Extract smartadId (integer) and set as tagid
            let smartad_id = bidder.get("smartadId")
                .map(|v| {
                    if let Some(n) = v.as_i64() { n.to_string() }
                    else if let Some(s) = v.as_str() { s.to_string() }
                    else { String::new() }
                })
                .unwrap_or_default();

            if smartad_id.is_empty() {
                errs.push(BidderError::BadInput(format!("imp {} missing smartadId", imp.id)));
                continue;
            }
            imp.tagid = Some(smartad_id);

            // Extract priceType (use first non-empty value)
            if price_type.is_empty() {
                if let Some(pt) = bidder.get("priceType").and_then(|v| v.as_str()) {
                    if !pt.is_empty() {
                        price_type = pt.to_string();
                    }
                }
            }

            valid_imps.push(imp);
        }

        if valid_imps.is_empty() {
            return (vec![], errs);
        }

        req.imp = valid_imps;

        // If priceType is set, inject into request.ext
        if !price_type.is_empty() {
            let mut ext_map: serde_json::Map<String, serde_json::Value> = req.ext.as_ref()
                .and_then(|e| serde_json::from_value(e.clone()).ok())
                .unwrap_or_default();
            ext_map.insert("pt".to_string(), serde_json::Value::String(price_type));
            req.ext = Some(serde_json::Value::Object(ext_map));
        }

        let body = match serde_json::to_vec(&req) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        (vec![RequestData {
            method: "POST".to_string(),
            uri: self.endpoint.clone(),
            body,
            headers,
            imp_ids: get_imp_ids(&req.imp),
        }], errs)
    }

    fn make_bids(&self, request: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code == 400 {
            return Err(vec![BidderError::BadInput("Unexpected status code: 400. Bad request from publisher.".to_string())]);
        }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(request.imp.len());
        if let Some(cur) = &bid_resp.cur {
            result.currency = cur.clone();
        }

        let mut errors = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                match get_bid_type_from_bid_ext(&bid) {
                    Ok(bid_type) => result.bids.push(TypedBid::new(bid, bid_type)),
                    Err(e) => errors.push(e),
                }
            }
        }

        if !errors.is_empty() {
            return Err(errors);
        }
        Ok(result)
    }
}
