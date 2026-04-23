use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use openrtb_ext::BidType;
use serde::{Deserialize, Serialize};

pub struct MissenaAdapter { pub endpoint: String }
impl MissenaAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

#[derive(Debug, Deserialize, Default)]
struct ExtImpMissena {
    #[serde(rename = "apiKey", default)]
    api_key: String,
    #[serde(default)]
    formats: Vec<String>,
    #[serde(default)]
    placement: String,
    #[serde(default)]
    sample: String,
    #[serde(default)]
    settings: Option<serde_json::Value>,
}

#[allow(dead_code)]
#[derive(Debug, Serialize, Clone, Default)]
struct EidUid {
    id: String,
    #[serde(rename = "atype", skip_serializing_if = "Option::is_none")]
    atype: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ext: Option<serde_json::Value>,
}

#[allow(dead_code)]
#[derive(Debug, Serialize, Clone, Default)]
struct Eid {
    source: String,
    uids: Vec<EidUid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ext: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Default)]
struct UserParams {
    #[serde(rename = "apiKey", skip_serializing_if = "String::is_empty")]
    api_key: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    formats: Vec<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    placement: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    sample: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    settings: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct MissenaAdRequest {
    #[serde(skip_serializing_if = "String::is_empty")]
    adunit: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    currency: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    debug: bool,
    #[serde(skip_serializing_if = "is_zero_f64")]
    floor: f64,
    #[serde(rename = "floor_currency", skip_serializing_if = "String::is_empty")]
    floor_currency: String,
    #[serde(rename = "ik", skip_serializing_if = "String::is_empty")]
    idempotency_key: String,
    #[serde(rename = "request_id", skip_serializing_if = "String::is_empty")]
    request_id: String,
    #[serde(skip_serializing_if = "is_zero_i64")]
    timeout: i64,
    params: UserParams,
    #[serde(rename = "userEids", skip_serializing_if = "Vec::is_empty")]
    user_eids: Vec<serde_json::Value>,
    ortb2: serde_json::Value,
    #[serde(skip_serializing_if = "String::is_empty")]
    version: String,
}

fn is_zero_f64(v: &f64) -> bool { *v == 0.0 }
fn is_zero_i64(v: &i64) -> bool { *v == 0 }

#[derive(Debug, Deserialize)]
struct BidServerResponse {
    #[serde(default)]
    cpm: f64,
    #[serde(default)]
    ad: String,
    #[serde(rename = "requestId", default)]
    request_id: String,
    #[serde(default)]
    currency: String,
}

/// Parse "scheme://host" from a URL string, ignoring malformed URLs.
fn parse_origin(page: &str) -> Option<String> {
    // Find "://" to split scheme from rest
    let sep = page.find("://")?;
    let scheme = &page[..sep];
    let rest = &page[sep + 3..];
    // Host ends at '/', '?', '#', or end of string
    let host_end = rest.find(|c| c == '/' || c == '?' || c == '#').unwrap_or(rest.len());
    let host = &rest[..host_end];
    if host.is_empty() { return None; }
    Some(format!("{}://{}", scheme, host))
}

fn get_currency<'a>(currencies: &'a [String]) -> &'a str {
    let mut eur_available = false;
    for cur in currencies {
        if cur == "USD" { return "USD"; }
        if cur == "EUR" { eur_available = true; }
    }
    if eur_available { "EUR" } else { "USD" }
}

impl Bidder for MissenaAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _info: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();

        let imp = match request.imp.first() {
            Some(i) => i,
            None => return (vec![], vec![BidderError::BadInput("no impressions".to_string())]),
        };

        let bidder_val = match imp.ext.as_ref().and_then(|e| e.get("bidder")) {
            Some(v) => v.clone(),
            None => {
                errs.push(BidderError::BadInput("Error parsing bidderExt object: missing bidder".to_string()));
                return (vec![], errs);
            }
        };
        let missena_ext: ExtImpMissena = match serde_json::from_value(bidder_val) {
            Ok(v) => v,
            Err(_) => {
                errs.push(BidderError::BadInput("Error parsing missenaExt parameters".to_string()));
                return (vec![], errs);
            }
        };

        let uri = self.endpoint.replace("{{.PublisherID}}", &missena_ext.api_key);

        let empty_cur: Vec<String> = Vec::new();
        let cur_slice = request.cur.as_deref().unwrap_or(&empty_cur);
        let cur = get_currency(cur_slice).to_string();

        let (floor, floor_cur) = if imp.bidfloor.unwrap_or(0.0) != 0.0 {
            let fc = get_currency(cur_slice).to_string();
            (imp.bidfloor.unwrap_or(0.0), fc)
        } else {
            (0.0, String::new())
        };

        // Extract EIDs from user.ext.eids (optional; ignored on error)
        let user_eids: Vec<serde_json::Value> = request.user.as_ref()
            .and_then(|u| u.ext.as_ref())
            .and_then(|e| e.get("eids"))
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let ortb2_val = match serde_json::to_value(request) {
            Ok(v) => v,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let version = "prebid-server@unknown".to_string();

        let missena_request = MissenaAdRequest {
            adunit: imp.id.clone(),
            currency: cur,
            debug: request.test.unwrap_or(0) == 1,
            floor,
            floor_currency: floor_cur,
            idempotency_key: request.id.clone(),
            request_id: request.id.clone(),
            timeout: request.tmax.unwrap_or(0),
            params: UserParams {
                api_key: missena_ext.api_key,
                formats: missena_ext.formats,
                placement: missena_ext.placement,
                sample: missena_ext.sample,
                settings: missena_ext.settings,
            },
            user_eids,
            ortb2: ortb2_val,
            version,
        };

        let body = match serde_json::to_vec(&missena_request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        if let Some(device) = &request.device {
            if let Some(ua) = &device.ua { if !ua.is_empty() { headers.insert("User-Agent".to_string(), ua.clone()); } }
            if let Some(ip) = &device.ip { if !ip.is_empty() { headers.insert("X-Forwarded-For".to_string(), ip.clone()); } }
            else if let Some(ip6) = &device.ipv6 { if !ip6.is_empty() { headers.insert("X-Forwarded-For".to_string(), ip6.clone()); } }
        }
        if let Some(site) = &request.site {
            if let Some(page) = &site.page {
                if !page.is_empty() {
                    headers.insert("Referer".to_string(), page.clone());
                    // Derive Origin from scheme + host of page URL
                    if let Some(origin) = parse_origin(page) {
                        headers.insert("Origin".to_string(), origin);
                    }
                }
            }
        }

        (vec![RequestData {
            method: "POST".to_string(),
            uri,
            body,
            headers,
            imp_ids: vec![imp.id.clone()],
        }], errs)
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code == 400 {
            return Err(vec![BidderError::BadInput(
                "Unexpected status code: 400. Bad request from publisher. Run with request.debug = 1 for more info.".to_string()
            )]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info.",
                response.status_code
            ))]);
        }
        let missena_resp: BidServerResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(1);
        result.currency = missena_resp.currency;
        let imp_id = internal.imp.first().map(|i| i.id.clone()).unwrap_or_default();
        let bid = openrtb::Bid {
            id: internal.id.clone(),
            price: missena_resp.cpm,
            impid: imp_id,
            adm: Some(missena_resp.ad),
            crid: Some(missena_resp.request_id),
            ..Default::default()
        };
        result.bids.push(TypedBid::new(bid, BidType::Banner));
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_origin() {
        assert_eq!(parse_origin("https://example.com/path").as_deref(), Some("https://example.com"));
        assert_eq!(parse_origin("not-a-url"), None);
    }

    #[test]
    fn test_make_requests_replaces_publisher_id_macro() {
        let adapter = MissenaAdapter::new("https://missena.example/{{.PublisherID}}/rtb".to_string());
        let mut req = openrtb::BidRequest::default();
        req.id = "r".to_string();
        req.imp = vec![openrtb::Imp {
            id: "imp1".to_string(),
            banner: Some(Default::default()),
            ext: Some(serde_json::json!({"bidder":{"apiKey":"KEY","placement":"sticky"}})),
            ..Default::default()
        }];
        let info = ExtraRequestInfo::default();
        let (requests, errs) = adapter.make_requests(&req, &info);
        assert!(errs.is_empty());
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].uri, "https://missena.example/KEY/rtb");
    }
}
