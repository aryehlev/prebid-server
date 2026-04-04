use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::{Deserialize, Serialize};

pub struct ShowheroesAdapter { pub endpoint: String }
impl ShowheroesAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

/// Bidder ext params for ShowHeroes
#[derive(Debug, Deserialize, Default)]
struct ExtImpShowheroes {
    #[serde(rename = "unitId", default)]
    unit_id: String,
}

/// Outgoing imp.ext: move bidder.unitId into params.unitId
#[derive(Debug, Serialize)]
struct ShImpExtOut {
    #[serde(skip_serializing_if = "Option::is_none")]
    prebid: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    gpid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
    params: ShImpParams,
}

#[derive(Debug, Serialize, Default)]
struct ShImpParams {
    #[serde(rename = "unitId", skip_serializing_if = "str::is_empty")]
    unit_id: String,
}

fn get_bid_type(mtype: i32) -> BidType {
    match mtype {
        1 => BidType::Banner,
        2 => BidType::Video,
        _ => BidType::Video, // default to video
    }
}

fn parse_imp_ext(imp: &openrtb::Imp) -> Result<ExtImpShowheroes, BidderError> {
    let ext = imp.ext.as_ref().ok_or_else(|| BidderError::BadInput("missing imp.ext".to_string()))?;
    let bidder_val = ext.get("bidder").ok_or_else(|| BidderError::BadInput("Error parsing bidder params".to_string()))?;
    serde_json::from_value(bidder_val.clone()).map_err(|_| BidderError::BadInput("Error parsing bidder params".to_string()))
}

/// Extract prebid channel name and version from request.ext.prebid.channel
fn get_prebid_channel(request: &openrtb::BidRequest) -> (String, String) {
    let ext = match request.ext.as_ref() {
        Some(e) => e,
        None => return (String::new(), String::new()),
    };
    let name = ext
        .get("prebid")
        .and_then(|p| p.get("channel"))
        .and_then(|c| c.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let version = ext
        .get("prebid")
        .and_then(|p| p.get("channel"))
        .and_then(|c| c.get("version"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    (name, version)
}

/// Set source.ext.pbs with pbsv/pbsp fields (Rust adapter sets pbsp="rust")
fn set_pbs_version(request: &mut openrtb::BidRequest) {
    let source = request.source.get_or_insert_with(Default::default);
    let mut ext_map: serde_json::Map<String, serde_json::Value> = match source.ext.as_ref() {
        Some(v) => {
            if let serde_json::Value::Object(m) = v.clone() { m }
            else { serde_json::Map::new() }
        }
        None => serde_json::Map::new(),
    };
    ext_map.insert(
        "pbs".to_string(),
        serde_json::json!({"pbsv": "unknown", "pbsp": "rust"}),
    );
    source.ext = Some(serde_json::Value::Object(ext_map));
}

impl Bidder for ShowheroesAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        // Validate: site must have page, app must have bundle
        if let Some(site) = &request.site {
            if site.page.as_deref().unwrap_or("").is_empty() {
                return (vec![], vec![BidderError::BadInput("site request doesn't have a page URL".to_string())]);
            }
        }
        if let Some(app) = &request.app {
            if app.bundle.as_deref().unwrap_or("").is_empty() {
                return (vec![], vec![BidderError::BadInput("app request doesn't have a bundle ID".to_string())]);
            }
        }
        if request.site.is_none() && request.app.is_none() {
            return (vec![], vec![BidderError::BadInput("request must contain a site or an app".to_string())]);
        }

        let (channel_name, channel_version) = get_prebid_channel(request);

        let mut errs = Vec::new();
        let mut valid_imps: Vec<openrtb::Imp> = Vec::with_capacity(request.imp.len());

        for imp in &request.imp {
            let sh_ext = match parse_imp_ext(imp) {
                Ok(e) => e,
                Err(e) => { errs.push(e); continue; }
            };

            // Build outgoing ext: copy prebid/gpid/tid/data from original, set params.unitId
            let orig_ext = imp.ext.as_ref().unwrap(); // safe since parse_imp_ext succeeded
            let out_ext = ShImpExtOut {
                prebid: orig_ext.get("prebid").cloned(),
                gpid: orig_ext.get("gpid").and_then(|v| v.as_str()).map(|s| s.to_string()),
                tid: orig_ext.get("tid").and_then(|v| v.as_str()).map(|s| s.to_string()),
                data: orig_ext.get("data").cloned(),
                params: ShImpParams { unit_id: sh_ext.unit_id },
            };

            let new_ext = match serde_json::to_value(&out_ext) {
                Ok(v) => v,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };

            let mut imp_copy = imp.clone();
            imp_copy.ext = Some(new_ext);

            // Set displayManager from prebid channel if not already set
            if imp_copy.displaymanager.as_deref().unwrap_or("").is_empty() {
                if !channel_name.is_empty() {
                    imp_copy.displaymanager = Some(channel_name.clone());
                }
                if !channel_version.is_empty() {
                    imp_copy.displaymanagerver = Some(channel_version.clone());
                }
            }

            valid_imps.push(imp_copy);
        }

        if valid_imps.is_empty() {
            return (vec![], errs);
        }

        let mut req_copy = request.clone();
        req_copy.imp = valid_imps;

        // Set source.ext.pbs version info
        set_pbs_version(&mut req_copy);

        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => { errs.push(BidderError::BadInput(e.to_string())); return (vec![], errs); }
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

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
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(5);
        if let Some(cur) = &bid_resp.cur { if !cur.is_empty() { result.currency = cur.clone(); } }
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = get_bid_type(bid.mtype.unwrap_or(0));
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        Ok(result)
    }
}
