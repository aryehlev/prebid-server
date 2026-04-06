use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;

pub struct SparteoAdapter { pub endpoint: String }
impl SparteoAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

/// Bidder ext for Sparteo
#[derive(Debug, Deserialize, Default)]
struct ExtImpSparteo {
    #[serde(rename = "networkId", default)]
    network_id: String,
}

fn parse_ext(imp: &openrtb::Imp) -> Result<ExtImpSparteo, BidderError> {
    let ext = imp.ext.as_ref()
        .ok_or_else(|| BidderError::BadInput(format!("ignoring imp id={}, error while decoding extImpBidder, err: missing ext", imp.id)))?;
    let bidder_val = ext.get("bidder")
        .ok_or_else(|| BidderError::BadInput(format!("ignoring imp id={}, error while decoding extImpBidder, err: missing bidder", imp.id)))?;
    serde_json::from_value(bidder_val.clone())
        .map_err(|e| BidderError::BadInput(format!("ignoring imp id={}, error while decoding impExt, err: {}", imp.id, e)))
}

fn get_media_type(bid: &openrtb::Bid) -> Result<BidType, BidderError> {
    let t = bid.ext.as_ref()
        .and_then(|e| e.get("prebid"))
        .and_then(|p| p.get("type"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    match t {
        "banner" => Ok(BidType::Banner),
        "video" => Ok(BidType::Video),
        "native" => Ok(BidType::Native),
        "audio" => Err(BidderError::BadServerResponse(format!("bid type \"audio\" is not supported for bid id={}", bid.id))),
        other => Err(BidderError::BadServerResponse(format!("error parsing bid type for bid id={}: {}", bid.id, other))),
    }
}

impl Bidder for SparteoAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut req_copy = request.clone();
        let mut errs = Vec::new();
        let mut site_network_id = String::new();

        // Process each imp: move ext.bidder fields into ext.sparteo.params, track networkId
        for i in 0..req_copy.imp.len() {
            let imp = &req_copy.imp[i];

            let ext_imp = match parse_ext(imp) {
                Ok(e) => e,
                Err(e) => { errs.push(e); continue; }
            };

            if site_network_id.is_empty() && !ext_imp.network_id.is_empty() {
                site_network_id = ext_imp.network_id.clone();
            }

            // Get ext as a mutable map
            let mut ext_map: serde_json::Map<String, serde_json::Value> = match imp.ext.as_ref() {
                Some(v) => {
                    if let serde_json::Value::Object(m) = v.clone() { m }
                    else {
                        errs.push(BidderError::BadInput(format!("ignoring imp id={}, error while unmarshaling ext, err: not an object", imp.id)));
                        continue;
                    }
                }
                None => serde_json::Map::new(),
            };

            // Remove ext.bidder first (before any mutable borrow of ext_map entries)
            let bidder_fields: Vec<(String, serde_json::Value)> =
                if let Some(serde_json::Value::Object(bidder_obj)) = ext_map.remove("bidder") {
                    bidder_obj.into_iter().collect()
                } else {
                    vec![]
                };

            // Get or create ext.sparteo
            let sparteo_obj = ext_map
                .entry("sparteo".to_string())
                .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
            let sparteo_map = if let serde_json::Value::Object(ref mut m) = sparteo_obj { m }
                else {
                    *sparteo_obj = serde_json::Value::Object(serde_json::Map::new());
                    if let serde_json::Value::Object(ref mut m) = sparteo_obj { m } else { unreachable!() }
                };

            // Get or create ext.sparteo.params
            let params_obj = sparteo_map
                .entry("params".to_string())
                .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
            let params_map = if let serde_json::Value::Object(ref mut m) = params_obj { m }
                else {
                    *params_obj = serde_json::Value::Object(serde_json::Map::new());
                    if let serde_json::Value::Object(ref mut m) = params_obj { m } else { unreachable!() }
                };

            // Move bidder fields into ext.sparteo.params
            for (key, value) in bidder_fields {
                params_map.insert(key, value);
            }

            let updated_ext = match serde_json::to_value(&ext_map) {
                Ok(v) => v,
                Err(e) => {
                    errs.push(BidderError::BadInput(format!("ignoring imp id={}, error while marshaling updated ext, err: {}", req_copy.imp[i].id, e)));
                    continue;
                }
            };

            req_copy.imp[i].ext = Some(updated_ext);
        }

        // Set site.publisher.ext.params.networkId if we found one
        if !site_network_id.is_empty() {
            if let Some(site) = req_copy.site.as_mut() {
                let publisher = site.publisher.get_or_insert_with(Default::default);

                let mut pub_ext: serde_json::Map<String, serde_json::Value> = match publisher.ext.as_ref() {
                    Some(v) => {
                        if let serde_json::Value::Object(m) = v.clone() { m }
                        else { serde_json::Map::new() }
                    }
                    None => serde_json::Map::new(),
                };

                let params_obj = pub_ext
                    .entry("params".to_string())
                    .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
                if let serde_json::Value::Object(ref mut pmap) = params_obj {
                    pmap.insert("networkId".to_string(), serde_json::Value::String(site_network_id.clone()));
                }

                match serde_json::to_value(&pub_ext) {
                    Ok(v) => publisher.ext = Some(v),
                    Err(e) => errs.push(BidderError::BadInput(format!("Error marshaling site.publisher.ext: {}", e))),
                }
            }
        }

        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => { errs.push(BidderError::BadInput(e.to_string())); return (vec![], errs); }
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json".to_string());

        (vec![RequestData {
            method: "POST".to_string(),
            uri: self.endpoint.clone(),
            body,
            headers,
            imp_ids: get_imp_ids(&req_copy.imp),
        }], errs)
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let mut bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(5);
        if let Some(cur) = &bid_resp.cur { if !cur.is_empty() { result.currency = cur.clone(); } }
        let mut errs = Vec::new();
        for sb in &mut bid_resp.seatbid {
            for bid in &mut sb.bid {
                match get_media_type(bid) {
                    Ok(bid_type) => {
                        // Set mtype from bid_type (as Go does)
                        bid.mtype = match bid_type {
                            BidType::Banner => Some(1),
                            BidType::Video => Some(2),
                            BidType::Native => Some(4),
                            _ => bid.mtype,
                        };
                        result.bids.push(TypedBid::new(bid.clone(), bid_type));
                    }
                    Err(e) => errs.push(e),
                }
            }
        }
        if !errs.is_empty() && result.bids.is_empty() { return Err(errs); }
        Ok(result)
    }
}
