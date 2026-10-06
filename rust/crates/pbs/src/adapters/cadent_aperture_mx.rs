//! Go `adapters/cadent_aperture_mx/cadentaperturemx.go`.

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

use crate::ortb::adcom1::MediaCreativeSubtype;

#[derive(Deserialize, Default)]
struct ExtImpCadentApertureMx {
    #[serde(default, alias = "tagId", alias = "TagID", alias = "TAGID")]
    tagid: String,
    #[serde(default)]
    bidfloor: String,
}

pub struct Adapter {
    endpoint: String,
    /// Go `adapter.testing`; the Go test flips it so URLs are deterministic.
    testing: bool,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self { endpoint: endpoint.into(), testing: false }
    }

    /// Go test helper `setTesting`.
    pub fn set_testing(&mut self, testing: bool) {
        self.testing = testing;
    }
}

fn build_endpoint(endpoint: &str, testing: bool, timeout: i64) -> String {
    let timeout = if timeout == 0 { 1000 } else { timeout };
    if testing {
        // for passing validation tests
        return format!("{endpoint}?t=1000&ts=2060541160");
    }
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{endpoint}?t={timeout}&ts={ts}&src=pbserver")
}

fn add_header_if_non_empty(headers: &mut Header, name: &str, value: &str) {
    if !value.is_empty() {
        headers.add(name, value);
    }
}

/// Go `strconv.ParseInt(s, 10, 64)` succeeded.
fn parse_int(s: &str) -> Option<i64> {
    s.parse::<i64>().ok()
}

fn unpack_imp_ext(imp: &Imp) -> Result<ExtImpCadentApertureMx, BidderError> {
    let bidder = imp_bidder_raw(&imp.ext).map_err(|e| BidderError::bad_input(e.to_string()))?;
    let ext: ExtImpCadentApertureMx = unmarshal_raw(&bidder)
        .map_err(|_| BidderError::bad_input(format!("ignoring imp id={}, invalid ImpExt", imp.id)))
        .and_then(|e| {
            check_string_fields(&bidder, "x", &[("tagid", "TagID"), ("bidfloor", "BidFloor")])
                .map_err(|_| BidderError::bad_input(format!("ignoring imp id={}, invalid ImpExt", imp.id)))?;
            Ok(e)
        })?;
    match parse_int(&ext.tagid) {
        Some(v) if v != 0 => {}
        _ => {
            return Err(BidderError::bad_input(format!(
                "ignoring imp id={}, invalid tagid must be a String of numbers",
                imp.id
            )))
        }
    }
    if ext.tagid.is_empty() {
        return Err(BidderError::bad_input(format!("Ignoring imp id={}, no tagid present", imp.id)));
    }
    Ok(ext)
}

fn build_imp_banner(imp: &mut Imp) -> Result<(), BidderError> {
    let Some(banner) = imp.banner.as_mut() else {
        return Err(BidderError::bad_input("Request needs to include a Banner object"));
    };
    if banner.w.is_none() && banner.h.is_none() {
        if banner.format.is_empty() {
            return Err(BidderError::bad_input("Need at least one size to build request"));
        }
        let format = banner.format.remove(0);
        banner.w = Some(format.w);
        banner.h = Some(format.h);
    }
    Ok(())
}

fn build_imp_video(imp: &mut Imp) -> Result<(), BidderError> {
    let Some(video) = imp.video.as_mut() else { return Ok(()) };
    if video.mimes.as_ref().map_or(true, Vec::is_empty) {
        return Err(BidderError::bad_input("Video: missing required field mimes"));
    }
    if video.h.unwrap_or_default() == 0 && video.w.unwrap_or_default() == 0 {
        return Err(BidderError::bad_input("Video: Need at least one size to build request"));
    }
    if !video.protocols.is_empty() {
        // not supporting VAST protocol 7 (VAST 4.0)
        video.protocols.retain(|p| *p != MediaCreativeSubtype::VAST_40);
    }
    Ok(())
}

fn add_imp_props(imp: &mut Imp, secure: i8, ext: &ExtImpCadentApertureMx) {
    imp.tagid = ext.tagid.clone();
    imp.secure = Some(secure);
    if !ext.bidfloor.is_empty() {
        let bid_floor = ext.bidfloor.parse::<f64>().unwrap_or(0.0);
        if bid_floor > 0.0 {
            imp.bidfloor = bid_floor;
            imp.bidfloorcur = "USD".into();
        }
    }
}

/// Go `preprocess`: rewrites `request.imp` to the valid imps; returns the errors.
fn preprocess(request: &mut BidRequest) -> Vec<BidderError> {
    let mut errors = vec![];
    let mut secure = 0i8;
    let mut domain = String::new();
    if let Some(site) = request.site.as_ref().filter(|s| !s.page.is_empty()) {
        domain = site.page.clone();
    } else if let Some(app) = &request.app {
        if !app.domain.is_empty() {
            domain = app.domain.clone();
        } else if !app.storeurl.is_empty() {
            domain = app.storeurl.clone();
        }
    }
    if let Ok(u) = url::Url::parse(&domain) {
        if u.scheme() == "https" {
            secure = 1;
        }
    }

    let imps = std::mem::take(&mut request.imp);
    let mut res = Vec::with_capacity(imps.len());
    for mut imp in imps {
        let ext = match unpack_imp_ext(&imp) {
            Ok(e) => e,
            Err(e) => {
                errors.push(e);
                continue;
            }
        };
        add_imp_props(&mut imp, secure, &ext);
        if imp.video.is_some() {
            if let Err(e) = build_imp_video(&mut imp) {
                errors.push(e);
                continue;
            }
        } else if let Err(e) = build_imp_banner(&mut imp) {
            errors.push(e);
            continue;
        }
        res.push(imp);
    }
    request.imp = res;
    errors
}

impl Bidder for Adapter {
    fn make_requests(&self, request: &BidRequest, _req_info: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::bad_input("No Imps in Bid Request")]);
        }
        let mut request = request.clone();
        let mut errs = preprocess(&mut request);
        if !errs.is_empty() {
            // Go formats the error slice with `%s`: `[a b]`.
            let joined = errs.iter().map(ToString::to_string).collect::<Vec<_>>().join(" ");
            errs.push(BidderError::bad_input(format!("Error in preprocess of Imp, err: [{joined}]")));
            return (vec![], errs);
        }
        let data = match marshal(&request) {
            Ok(d) => d,
            Err(_) => return (vec![], vec![BidderError::bad_input("Error in packaging request to JSON")]),
        };
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        if let Some(device) = &request.device {
            add_header_if_non_empty(&mut headers, "User-Agent", &device.ua);
            add_header_if_non_empty(&mut headers, "X-Forwarded-For", &device.ip);
            add_header_if_non_empty(&mut headers, "Accept-Language", &device.language);
            if let Some(dnt) = device.dnt {
                add_header_if_non_empty(&mut headers, "DNT", &dnt.to_string());
            }
        }
        if let Some(site) = &request.site {
            add_header_if_non_empty(&mut headers, "Referer", &site.page);
        }
        let url = build_endpoint(&self.endpoint, self.testing, request.tmax);
        (
            vec![RequestData { method: "POST".into(), uri: url, body: data, headers, imp_ids: imp_ids(&request.imp) }],
            errs,
        )
    }

    fn make_bids(
        &self,
        _request: &BidRequest,
        _request_data: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if response.status_code == 204 {
            return (None, vec![]);
        }
        if response.status_code != 200 {
            return (
                None,
                vec![BidderError::bad_server_response(format!(
                    "Invalid Status Returned: {}. Run with request.debug = 1 for more info",
                    response.status_code
                ))],
            );
        }
        // The shared decoder accepts numeric strings where Go's jsoniter fails; reproduce the Go
        // failure for bid `w`/`h` given as strings.
        let strict_err = strict_bid_dims(&response.body);
        let bid_resp: Result<BidResponse, BidderError> = match strict_err {
            Some(e) => Err(e),
            None => jsonutil::unmarshal(&response.body),
        };
        let bid_resp: BidResponse = match bid_resp {
            Ok(r) => r,
            Err(e) => {
                return (
                    None,
                    vec![BidderError::bad_server_response(format!("Unable to unpackage bid response. Error: {e}"))],
                )
            }
        };
        let mut bid_response = BidderResponse::with_bids_capacity(1);
        for sb in bid_resp.seatbid {
            for mut bid in sb.bid {
                bid.impid = bid.id.clone();
                let t = get_bid_type(&bid.adm);
                bid_response.bids.push(TypedBid::new(bid, t));
            }
        }
        (Some(bid_response), vec![])
    }
}

fn get_bid_type(adm: &str) -> BidType {
    if !adm.is_empty() && contains_any(adm, &["<?xml", "<vast"]) {
        return BidType::Video;
    }
    BidType::Banner
}

fn contains_any(raw: &str, keys: &[&str]) -> bool {
    let lower = raw.to_lowercase();
    keys.iter().any(|k| lower.contains(k))
}

/// Go: `cannot unmarshal openrtb2.Bid.W: unexpected character` for a string `w`/`h` in a bid.
fn strict_bid_dims(body: &[u8]) -> Option<BidderError> {
    let v: serde_json::Value = serde_json::from_slice(body).ok()?;
    for sb in v.get("seatbid")?.as_array()? {
        for bid in sb.get("bid")?.as_array()? {
            for (key, field) in [("w", "W"), ("h", "H")] {
                if bid.get(key).is_some_and(serde_json::Value::is_string) {
                    return Some(BidderError::FailedToUnmarshal(format!(
                        "cannot unmarshal openrtb2.Bid.{field}: unexpected character"
                    )));
                }
            }
        }
    }
    None
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
