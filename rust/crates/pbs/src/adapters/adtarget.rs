//! Go `adapters/adtarget/adtarget.go`.

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

/// Go `openrtb_ext.ExtImpAdtarget`; `aid` is a `json.Number` (kept as its literal text).
#[derive(Deserialize, Default)]
struct ExtImpAdtarget {
    #[serde(default)]
    aid: NumberLit,
    #[serde(rename = "placementId", default)]
    placement_id: i64,
    #[serde(rename = "siteId", default)]
    site_id: i64,
    #[serde(rename = "bidFloor", default)]
    bid_floor: f64,
}

/// Go `json.Number`: accepts a JSON number or a string, keeps the text.
#[derive(Default, Clone)]
struct NumberLit(String);

impl<'de> Deserialize<'de> for NumberLit {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = serde_json::Value::deserialize(d)?;
        match v {
            serde_json::Value::Null => Ok(Self(String::new())),
            serde_json::Value::Number(n) => Ok(Self(n.to_string())),
            serde_json::Value::String(s) => Ok(Self(s)),
            other => Err(serde::de::Error::custom(format!("invalid json.Number: {other}"))),
        }
    }
}

fn is_number_literal(s: &str) -> bool {
    let b = s.as_bytes();
    let mut i = 0;
    if i < b.len() && b[i] == b'-' {
        i += 1;
    }
    let ds = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    if i == ds || (b[ds] == b'0' && i - ds > 1) {
        return false;
    }
    if i < b.len() && b[i] == b'.' {
        i += 1;
        let fs = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == fs {
            return false;
        }
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        i += 1;
        if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
            i += 1;
        }
        let es = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == es {
            return false;
        }
    }
    i == b.len()
}

pub struct Adapter {
    endpoint: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self { endpoint: endpoint.into() }
    }
}

/// Go `validateImpressionAndSetExt`: mutates `imp`, returns the source id.
fn validate_impression_and_set_ext(imp: &mut Imp) -> Result<i64, BidderError> {
    if imp.banner.is_none() && imp.video.is_none() {
        return Err(BidderError::bad_input(format!(
            "ignoring imp id={}, Adtarget supports only Video and Banner",
            imp.id
        )));
    }
    if imp.ext.is_none() {
        return Err(BidderError::bad_input(format!("ignoring imp id={}, extImpBidder is empty", imp.id)));
    }
    let bidder = imp_bidder_raw(&imp.ext).map_err(|e| {
        BidderError::bad_input(format!("ignoring imp id={}, error while decoding extImpBidder, err: {}", imp.id, e))
    })?;
    let ext: ExtImpAdtarget = unmarshal_raw(&bidder).map_err(|e| {
        BidderError::bad_input(format!("ignoring imp id={}, error while decoding impExt, err: {}", imp.id, e))
    })?;

    // Marshal `{"adtarget": ext}` (json.Number: empty is `0`, invalid literal is an error).
    let aid_text = if ext.aid.0.is_empty() { "0".to_string() } else { ext.aid.0.clone() };
    if !is_number_literal(&aid_text) {
        return Err(BidderError::bad_input(format!(
            "ignoring imp id={}, error while encoding impExt, err: json: invalid number literal \"{}\"",
            imp.id, aid_text
        )));
    }
    let mut inner = format!("{{\"aid\":{aid_text}");
    if ext.placement_id != 0 {
        inner.push_str(&format!(",\"placementId\":{}", ext.placement_id));
    }
    if ext.site_id != 0 {
        inner.push_str(&format!(",\"siteId\":{}", ext.site_id));
    }
    if ext.bid_floor != 0.0 {
        inner.push_str(&format!(",\"bidFloor\":{}", serde_json::to_string(&ext.bid_floor).unwrap_or_default()));
    }
    inner.push('}');
    let ext_json = format!("{{\"adtarget\":{inner}}}");
    if ext.bid_floor > 0.0 {
        imp.bidfloor = ext.bid_floor;
    }
    imp.ext = Some(Ext::from_slice(ext_json.as_bytes()).map_err(|e| BidderError::other(e.to_string()))?);

    // json.Number.Int64 on the original value: the marshal above wrote an empty Number as `0`,
    // but `SourceId` itself is still empty and `Int64()` fails on it.
    let original = ext.aid.0.as_str();
    original.parse::<i64>().map_err(|_| {
        BidderError::bad_input(format!(
            "ignoring imp id={}, aid parsing err: strconv.ParseInt: parsing \"{}\": invalid syntax",
            imp.id, original
        ))
    })
}

impl Bidder for Adapter {
    fn make_requests(&self, request: &BidRequest, _req_info: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errors = vec![];
        // Go ranges over a map keyed by source id (random order); first-seen order here.
        let mut groups: Vec<(i64, Vec<Imp>)> = vec![];
        for imp in &request.imp {
            let mut imp = imp.clone();
            match validate_impression_and_set_ext(&mut imp) {
                Err(e) => errors.push(e),
                Ok(source_id) => match groups.iter_mut().find(|(k, _)| *k == source_id) {
                    Some((_, v)) => v.push(imp),
                    None => groups.push((source_id, vec![imp])),
                },
            }
        }
        if groups.is_empty() {
            return (vec![], errors);
        }
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");

        let (base, _) = split_request(request);
        let mut reqs = vec![];
        for (source_id, imps) in groups {
            let mut req = base.clone();
            req.imp = imps;
            let body = match marshal(&req) {
                Ok(b) => b,
                Err(e) => {
                    errors.push(BidderError::other(format!("error while encoding bidRequest, err: {e}")));
                    return (vec![], errors);
                }
            };
            reqs.push(RequestData {
                method: "POST".into(),
                uri: format!("{}?aid={}", self.endpoint, source_id),
                body,
                headers: headers.clone(),
                imp_ids: imp_ids(&req.imp),
            });
        }
        (reqs, errors)
    }

    fn make_bids(
        &self,
        bid_req: &BidRequest,
        _unused: &RequestData,
        http_res: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if http_res.status_code == 204 {
            return (None, vec![]);
        }
        if http_res.status_code == 400 {
            return (
                None,
                vec![BidderError::bad_input(format!(
                    "Unexpected status code: {}. Run with request.debug = 1 for more info",
                    http_res.status_code
                ))],
            );
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&http_res.body) {
            Ok(r) => r,
            Err(e) => {
                return (None, vec![BidderError::bad_server_response(format!("error while decoding response, err: {e}"))])
            }
        };
        let mut bid_response = BidderResponse::new();
        let mut errors = vec![];
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let mut imp_ok = false;
                let mut media_type = BidType::Banner;
                for imp in &bid_req.imp {
                    if imp.id == bid.impid {
                        imp_ok = true;
                        if imp.video.is_some() {
                            media_type = BidType::Video;
                            break;
                        }
                    }
                }
                if !imp_ok {
                    errors.push(BidderError::bad_server_response(format!(
                        "ignoring bid id={}, request doesn't contain any impression with id={}",
                        bid.id, bid.impid
                    )));
                    continue;
                }
                bid_response.bids.push(TypedBid::new(bid, media_type));
            }
        }
        (Some(bid_response), errors)
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
