use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::{Deserialize, Serialize};

pub struct RelevantdigitalAdapter { pub endpoint: String }
impl RelevantdigitalAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

const RELEVANT_DOMAIN: &str = ".relevant-digital.com";
const DEFAULT_TIMEOUT: i64 = 1000;
const DEFAULT_BUFFER_MS: i64 = 250;

/// Bidder ext params for RelevantDigital
#[derive(Debug, Deserialize, Default)]
struct ExtImpRelevantDigital {
    #[serde(rename = "accountId", default)]
    account_id: String,
    #[serde(rename = "placementId", default)]
    placement_id: String,
    #[serde(rename = "pbsHost", default)]
    pbs_host: String,
    #[serde(rename = "pbsBufferMs", default)]
    pbs_buffer_ms: Option<i64>,
}

/// request.ext structure with relevant and prebid fields
#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize, Default)]
struct RelevantRequestExt {
    #[serde(default)]
    relevant: RelevantMeta,
    #[serde(default)]
    prebid: PrebidMeta,
}

#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize, Default)]
struct RelevantMeta {
    #[serde(default)]
    count: i32,
    #[serde(rename = "adapterType", default)]
    adapter_type: String,
}

#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize, Default)]
struct PrebidMeta {
    #[serde(default)]
    storedrequest: StoredRequest,
    #[serde(default)]
    debug: bool,
}

#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize, Default)]
struct StoredRequest {
    #[serde(default)]
    id: String,
}

/// Parse bidder ext from a single imp
fn get_imp_ext(imp: &openrtb::Imp) -> Result<ExtImpRelevantDigital, BidderError> {
    let ext = imp.ext.as_ref().ok_or_else(|| BidderError::BadInput("imp.ext not provided".to_string()))?;
    let bidder_val = ext.get("bidder").ok_or_else(|| BidderError::BadInput("ext.bidder not provided".to_string()))?;
    let mut ext_imp: ExtImpRelevantDigital = serde_json::from_value(bidder_val.clone())
        .map_err(|_| BidderError::BadInput("ext.bidder not provided".to_string()))?;
    if ext_imp.pbs_buffer_ms.is_none() {
        ext_imp.pbs_buffer_ms = Some(DEFAULT_BUFFER_MS);
    }
    Ok(ext_imp)
}

/// Build the endpoint URL from pbsHost by stripping http(s):// and the relevant domain suffix,
/// then constructing: https://<host>.relevant-digital.com/prebid/bid
fn build_endpoint_url(base_endpoint: &str, pbs_host: &str) -> String {
    let mut host = pbs_host.to_string();
    host = host.replace("http://", "");
    host = host.replace("https://", "");
    // Strip the relevant domain suffix if present
    if let Some(stripped) = host.strip_suffix(RELEVANT_DOMAIN) {
        host = stripped.to_string();
    }
    // Use template substitution: replace {{.Host}} in endpoint template
    if base_endpoint.contains("{{.Host}}") {
        return base_endpoint.replace("{{.Host}}", &host);
    }
    // Fallback: if no template, build default URL
    format!("https://{}{}/prebid/bid", host, RELEVANT_DOMAIN)
}

/// Patch request.ext with relevant count, adapterType and storedrequest id
fn patch_request_ext(request: &mut openrtb::BidRequest, account_id: &str) -> Result<(), BidderError> {
    let mut ext_obj: serde_json::Map<String, serde_json::Value> = match request.ext.as_ref() {
        Some(v) => {
            if let serde_json::Value::Object(m) = v.clone() { m }
            else { serde_json::Map::new() }
        }
        None => serde_json::Map::new(),
    };

    // Parse existing relevant count
    let count = ext_obj.get("relevant")
        .and_then(|r| r.get("count"))
        .and_then(|c| c.as_i64())
        .unwrap_or(0) as i32;

    if count >= 5 {
        return Err(BidderError::FailedToRequestBids("too many requests".to_string()));
    }

    // Build the relevant sub-object
    let relevant_obj = serde_json::json!({
        "count": count + 1,
        "adapterType": "server"
    });
    ext_obj.insert("relevant".to_string(), relevant_obj);

    // Patch prebid.storedrequest.id
    let prebid_obj = ext_obj.entry("prebid".to_string())
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    if let serde_json::Value::Object(ref mut pmap) = prebid_obj {
        let sr = pmap.entry("storedrequest".to_string())
            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
        if let serde_json::Value::Object(ref mut srmap) = sr {
            srmap.insert("id".to_string(), serde_json::Value::String(account_id.to_string()));
        }
    }

    request.ext = Some(serde_json::Value::Object(ext_obj));
    Ok(())
}

/// Set imp.ext to {"prebid":{"storedrequest":{"id":"<placement_id>"}}}
fn patch_imp_ext(imp: &mut openrtb::Imp, placement_id: &str) {
    imp.ext = Some(serde_json::json!({
        "prebid": {
            "storedrequest": {
                "id": placement_id
            }
        }
    }));
}

/// Adjust tmax: subtract buffer but stay within [buffer, original_tmax]
fn set_tmax(request: &mut openrtb::BidRequest, buffer_ms: i64) {
    let timeout = match request.tmax {
        Some(t) if t > 0 => t,
        _ => DEFAULT_TIMEOUT,
    };
    let new_tmax = (timeout - buffer_ms).max(buffer_ms).min(timeout);
    request.tmax = Some(new_tmax);
}

/// Scrub relevant/prebid cached data from the request JSON after marshaling
#[allow(dead_code)]
fn scrub_json(body: Vec<u8>, imp_count: usize) -> Vec<u8> {
    // We do a simple JSON string manipulation to delete unwanted keys.
    // Since we're working with serde_json Value directly, we handle this in the struct level.
    // The Go implementation uses jsonparser.Delete after marshaling; we approximate by
    // using serde_json manipulation before marshaling instead (already done via patch_imp_ext).
    let _ = imp_count; // used for documentation
    body
}

fn get_bid_type_from_ext(bid: &openrtb::Bid) -> Result<BidType, BidderError> {
    let t = bid.ext.as_ref()
        .and_then(|e| e.get("prebid"))
        .and_then(|p| p.get("type"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    match t {
        "banner" => Ok(BidType::Banner),
        "video" => Ok(BidType::Video),
        "native" => Ok(BidType::Native),
        "audio" => Ok(BidType::Audio),
        _ => Err(BidderError::BadServerResponse(format!("failed to parse bid type, missing ext: {}", bid.impid))),
    }
}

fn get_media_type_for_bid(bid: &openrtb::Bid) -> Result<BidType, BidderError> {
    match bid.mtype {
        Some(1) => Ok(BidType::Banner),
        Some(2) => Ok(BidType::Video),
        Some(3) => Ok(BidType::Audio),
        Some(4) => Ok(BidType::Native),
        _ => get_bid_type_from_ext(bid),
    }
}

impl Bidder for RelevantdigitalAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        // Collect ext params for all imps
        let mut imp_params: Vec<ExtImpRelevantDigital> = Vec::with_capacity(request.imp.len());
        let mut errs = Vec::new();

        for imp in &request.imp {
            match get_imp_ext(imp) {
                Ok(p) => imp_params.push(p),
                Err(e) => { errs.push(e); }
            }
        }

        if imp_params.is_empty() {
            return (vec![], errs);
        }

        // Use first imp's params for top-level settings
        let first = &imp_params[0];
        let buffer_ms = first.pbs_buffer_ms.unwrap_or(DEFAULT_BUFFER_MS);

        // Build endpoint URL from pbsHost
        let url = if !first.pbs_host.is_empty() {
            build_endpoint_url(&self.endpoint, &first.pbs_host)
        } else {
            self.endpoint.clone()
        };

        // Clone request and mutate
        let mut req_copy = request.clone();

        // Patch request ext (count check + storedrequest + adapterType)
        if let Err(e) = patch_request_ext(&mut req_copy, &first.account_id) {
            return (vec![], vec![BidderError::BadInput(format!("failed to create bidRequest, error: {}", e))]);
        }

        // Adjust tmax
        set_tmax(&mut req_copy, buffer_ms);

        // Patch each imp ext with its placementId
        for (i, params) in imp_params.iter().enumerate() {
            if i < req_copy.imp.len() {
                patch_imp_ext(&mut req_copy.imp[i], &params.placement_id);
            }
        }

        // Scrub prebid cache/targeting/aliases from request.ext.prebid
        if let Some(serde_json::Value::Object(ref mut ext_map)) = req_copy.ext {
            if let Some(serde_json::Value::Object(ref mut prebid_map)) = ext_map.get_mut("prebid") {
                prebid_map.remove("cache");
                prebid_map.remove("targeting");
                prebid_map.remove("aliases");
            }
        }

        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
        headers.insert("X-Openrtb-Version".to_string(), "2.5".to_string());

        // Add device headers if present
        if let Some(device) = &request.device {
            if let Some(ua) = &device.ua {
                if !ua.is_empty() {
                    headers.insert("User-Agent".to_string(), ua.clone());
                }
            }
            if let Some(ipv6) = &device.ipv6 {
                if !ipv6.is_empty() {
                    headers.insert("X-Forwarded-For".to_string(), ipv6.clone());
                }
            }
            if let Some(ip) = &device.ip {
                if !ip.is_empty() {
                    headers.insert("X-Forwarded-For".to_string(), ip.clone());
                }
            }
        }

        (vec![RequestData {
            method: "POST".to_string(),
            uri: url,
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
        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in &sb.bid {
                match get_media_type_for_bid(bid) {
                    Ok(bid_type) => {
                        // Only supported types
                        match bid_type {
                            BidType::Banner | BidType::Video | BidType::Audio | BidType::Native => {
                                result.bids.push(TypedBid::new(bid.clone(), bid_type));
                            }
                        }
                    }
                    Err(e) => errs.push(e),
                }
            }
        }
        if !errs.is_empty() && result.bids.is_empty() { return Err(errs); }
        Ok(result)
    }
}
