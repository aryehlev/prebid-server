//! Go `adapters/sovrn/sovrn.go`.

#![allow(unused_imports, dead_code)]
use crate::bid_types::BidType;
use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder, BidderResponse,
    ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::macros::{EndpointTemplate, EndpointTemplateParams};
use crate::ortb::openrtb2::{Bid, BidRequest, BidResponse, Imp, MarkupType};
use crate::ortb::Ext;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Default)]
struct ExtImpSovrn {
    #[serde(rename = "tagId", default)]
    tag_id: String,
    #[serde(default)]
    tagid: String,
    #[serde(default)]
    bidfloor: Option<serde_json::Value>,
}

pub struct Adapter {
    uri: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self { uri: endpoint.into() }
    }
}

fn add_header_if_non_empty(headers: &mut Header, name: &str, value: &str) {
    if !value.is_empty() {
        headers.add(name, value);
    }
}

fn get_ext_bid_floor(ext: &ExtImpSovrn) -> f64 {
    match &ext.bidfloor {
        Some(serde_json::Value::String(s)) => s.parse::<f64>().unwrap_or(0.0),
        Some(serde_json::Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        _ => 0.0,
    }
}

fn get_tag_id(ext: &ExtImpSovrn) -> String {
    if !ext.tagid.is_empty() {
        ext.tagid.clone()
    } else {
        ext.tag_id.clone()
    }
}

/// Go `url.QueryUnescape`.
fn query_unescape(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' => {
                let hex = b.get(i + 1..i + 3)?;
                let v = u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()?;
                out.push(v);
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    Some(String::from_utf8_lossy(&out).into_owned())
}

impl Bidder for Adapter {
    fn make_requests(&self, request: &BidRequest, _req_info: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json");
        if let Some(device) = &request.device {
            add_header_if_non_empty(&mut headers, "User-Agent", &device.ua);
            add_header_if_non_empty(&mut headers, "X-Forwarded-For", &device.ip);
            add_header_if_non_empty(&mut headers, "Accept-Language", &device.language);
            if let Some(dnt) = device.dnt {
                add_header_if_non_empty(&mut headers, "DNT", &dnt.to_string());
            }
        }
        if let Some(user) = &request.user {
            let user_id = user.buyeruid.trim();
            if !user_id.is_empty() {
                headers.add("Cookie", format!("ljt_reader={user_id}"));
            }
        }

        let mut errs = vec![];
        let mut valid_imps = Vec::with_capacity(request.imp.len());
        for imp in &request.imp {
            let bidder = match imp_bidder_raw(&imp.ext) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(BidderError::bad_input(e.to_string()));
                    continue;
                }
            };
            let sovrn_ext: ExtImpSovrn = match unmarshal_raw(&bidder) {
                Ok(e) => e,
                Err(e) => {
                    errs.push(BidderError::bad_input(e.to_string()));
                    continue;
                }
            };
            let tag_id = get_tag_id(&sovrn_ext);
            if tag_id.is_empty() {
                errs.push(BidderError::bad_input("Missing required parameter 'tagid'"));
                continue;
            }
            let mut imp = imp.clone();
            imp.tagid = tag_id;
            let ext_bid_floor = get_ext_bid_floor(&sovrn_ext);
            if imp.bidfloor == 0.0 && ext_bid_floor > 0.0 {
                imp.bidfloor = ext_bid_floor;
            }
            // Validate video params if appropriate. Go tests for nil slices; an empty slice is
            // treated the same here (the typed request cannot tell them apart).
            if let Some(video) = &imp.video {
                if video.mimes.is_none() || video.maxduration == 0 || video.protocols.is_empty() {
                    errs.push(BidderError::bad_input("Missing required video parameter"));
                    continue;
                }
            }
            valid_imps.push(imp);
        }
        if valid_imps.is_empty() {
            return (vec![], errs);
        }
        let mut req = request.clone();
        req.imp = valid_imps;
        let body = match marshal(&req) {
            Ok(b) => b,
            Err(e) => {
                errs.push(e);
                return (vec![], errs);
            }
        };
        (
            vec![RequestData { method: "POST".into(), uri: self.uri.clone(), body, headers, imp_ids: imp_ids(&req.imp) }],
            errs,
        )
    }

    fn make_bids(
        &self,
        request: &BidRequest,
        _request_data: &RequestData,
        bidder_response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        match bidder_response.status_code {
            204 => return (None, vec![]),
            400 => {
                return (
                    None,
                    vec![BidderError::bad_input(format!(
                        "Unexpected status code: {}. Run with request.debug = 1 for more info",
                        bidder_response.status_code
                    ))],
                )
            }
            200 => {}
            c => {
                return (
                    None,
                    vec![BidderError::bad_server_response(format!(
                        "Unexpected status code: {c}. Run with request.debug = 1 for more info"
                    ))],
                )
            }
        }
        let bid_response: BidResponse = match jsonutil::unmarshal(&bidder_response.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![BidderError::bad_server_response(e.to_string())]),
        };
        let mut response = BidderResponse::with_bids_capacity(5);
        let mut errs = vec![];
        for sb in bid_response.seatbid {
            for mut bid in sb.bid {
                // Go skips (silently) bids whose adm does not unescape.
                let Some(adm) = query_unescape(&bid.adm) else { continue };
                bid.adm = adm;
                let mut bid_type = BidType::Banner;
                match request.imp.iter().find(|imp| imp.id == bid.impid) {
                    None => {
                        errs.push(BidderError::bad_input(format!(
                            "Imp ID {} in bid didn't match with any imp in the original request",
                            bid.impid
                        )));
                        continue;
                    }
                    Some(imp) if imp.video.is_some() => bid_type = BidType::Video,
                    Some(_) => {}
                }
                response.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        (Some(response), errs)
    }
}

// ---- local helpers (Go `adapters.ExtImpBidder` + `jsonutil.Unmarshal` on raw ext bytes) ----

#[derive(Debug, Default, serde::Deserialize)]
struct ExtImpBidder {
    #[serde(default)]
    bidder: Option<Ext>,
}

/// Go `jsonutil.Unmarshal(raw, &v)` where `raw` may be empty (nil `json.RawMessage`): json-iterator
/// reports the NUL it reads past the end of the input. `null` leaves `v` at its zero value.
fn unmarshal_raw<T: serde::de::DeserializeOwned + Default>(raw: &[u8]) -> Result<T, BidderError> {
    if raw.is_empty() {
        return Err(BidderError::FailedToUnmarshal("expect { or n, but found \u{0}".to_string()));
    }
    if raw.iter().find(|b| !b" \t\r\n".contains(b)) == Some(&b'n') && raw.trim_ascii() == b"null" {
        return Ok(T::default());
    }
    jsonutil::unmarshal(raw)
}

fn ext_bytes(ext: &Option<Ext>) -> Vec<u8> {
    ext.as_ref().map(|e| e.to_json().into_bytes()).unwrap_or_default()
}

/// Go `jsonutil.Unmarshal(imp.Ext, &adapters.ExtImpBidder)`, returning `bidderExt.Bidder` bytes
/// (empty when absent).
fn imp_bidder_raw(ext: &Option<Ext>) -> Result<Vec<u8>, BidderError> {
    let parsed: ExtImpBidder = unmarshal_raw(&ext_bytes(ext))?;
    Ok(parsed.bidder.map(|b| b.to_json().into_bytes()).unwrap_or_default())
}

/// Go `json.Marshal(v)` into a `json.RawMessage` stand-in.
fn ext_from<T: serde::Serialize>(v: &T) -> Result<Ext, BidderError> {
    let bytes = crate::go_json::to_vec(v).map_err(|e| BidderError::other(e.to_string()))?;
    Ext::from_slice(&bytes).map_err(|e| BidderError::other(e.to_string()))
}

/// Go `openrtb_ext.GetImpIDs`.
fn imp_ids(imps: &[Imp]) -> Vec<String> {
    imps.iter().map(|i| i.id.clone()).collect()
}

/// Go `reqCopy := *request` then replacing `Imp`: a copy of the request without its imps, plus the
/// imps (cloned once).
fn split_request(request: &BidRequest) -> (BidRequest, Vec<Imp>) {
    let mut base = request.clone();
    let imps = std::mem::take(&mut base.imp);
    (base, imps)
}

/// Go `url.QueryEscape`.
fn query_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn marshal(request: &BidRequest) -> Result<Vec<u8>, BidderError> {
    crate::go_json::to_vec(request).map_err(|e| BidderError::other(e.to_string()))
}

/// jsoniter's struct-path message for a JSON string field holding a non-string value, e.g.
/// `cannot unmarshal openrtb_ext.ExtImpAJA.AdSpotID: expects " or n, but found 1`. Checked before
/// the serde decode, which words it differently. `fields` is `(json key, Go field name)`.
fn check_string_fields(raw: &[u8], go_struct: &str, fields: &[(&str, &str)]) -> Result<(), BidderError> {
    let Ok(serde_json::Value::Object(obj)) = serde_json::from_slice::<serde_json::Value>(raw) else {
        return Ok(());
    };
    for (key, go_field) in fields {
        if let Some(v) = obj.get(*key) {
            let found = match v {
                serde_json::Value::Null | serde_json::Value::String(_) => continue,
                serde_json::Value::Array(_) => '[',
                serde_json::Value::Object(_) => '{',
                serde_json::Value::Bool(true) => 't',
                serde_json::Value::Bool(false) => 'f',
                serde_json::Value::Number(n) => n.to_string().chars().next().unwrap_or('0'),
            };
            return Err(BidderError::FailedToUnmarshal(format!(
                "cannot unmarshal {go_struct}.{go_field}: expects \" or n, but found {found}"
            )));
        }
    }
    Ok(())
}
