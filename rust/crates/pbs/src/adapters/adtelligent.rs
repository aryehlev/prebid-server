//! Go `adapters/adtelligent/adtelligent.go`.

use serde::{Deserialize, Serialize};

use crate::bid_types::BidType;
use crate::bidder::{
    Bidder, BidderResponse,
    ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{BidRequest, BidResponse, Imp};
use crate::ortb::Ext;

/// Go `openrtb_ext.ExtImpAdtelligent`.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct ExtImpAdtelligent {
    #[serde(rename = "aid")]
    source_id: JsonNumberSer,
    #[serde(rename = "placementId", skip_serializing_if = "is_zero_i64")]
    placement_id: i64,
    #[serde(rename = "siteId", skip_serializing_if = "is_zero_i64")]
    site_id: i64,
    #[serde(rename = "bidFloor", skip_serializing_if = "is_zero_f64")]
    bid_floor: f64,
}

fn is_zero_i64(v: &i64) -> bool {
    *v == 0
}
fn is_zero_f64(v: &f64) -> bool {
    *v == 0.0
}

/// `json.Number` that also serializes (as the number text, or `0` when empty, as Go does).
#[derive(Debug, Default, Clone)]
struct JsonNumberSer(JsonNumber);

impl<'de> Deserialize<'de> for JsonNumberSer {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsonNumber::deserialize(d).map(Self)
    }
}

impl Serialize for JsonNumberSer {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let text = if self.0.as_str().is_empty() { "0" } else { self.0.as_str() };
        match serde_json::Number::from_string_unchecked_compat(text) {
            Some(n) => n.serialize(s),
            None => Err(serde::ser::Error::custom(format!("json: invalid number literal {text:?}"))),
        }
    }
}

trait NumberCompat: Sized {
    fn from_string_unchecked_compat(s: &str) -> Option<Self>;
}

impl NumberCompat for serde_json::Number {
    fn from_string_unchecked_compat(s: &str) -> Option<Self> {
        // Valid JSON number text only (Go's `json.Number` marshal check).
        match serde_json::from_str::<serde_json::Value>(s) {
            Ok(serde_json::Value::Number(n)) => Some(n),
            _ => None,
        }
    }
}

#[derive(Serialize)]
struct AdtelligentImpExt<'a> {
    adtelligent: &'a ExtImpAdtelligent,
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

// ---- local helpers (shared foundation files are off limits) ----

/// Go `jsonutil.Unmarshal(ext, &v)` on an optional raw message: a missing message is empty
/// input (`expect { or n, but found` + NUL), anything but an object or null is rejected with
/// json-iterator's top-level wording.
#[allow(dead_code)]
fn decode_ext<T: serde::de::DeserializeOwned>(ext: Option<&Ext>) -> Result<T, BidderError> {
    let Some(ext) = ext else {
        return Err(BidderError::FailedToUnmarshal("expect { or n, but found \u{0}".into()));
    };
    {
        use sonic_rs::JsonValueTrait;
        if !ext.0.is_object() && !ext.0.is_null() {
            let found = ext.to_json().chars().next().unwrap_or('\u{0}');
            return Err(BidderError::FailedToUnmarshal(format!("expect {{ or n, but found {found}")));
        }
    }
    ext.decode::<T>().map_err(|e| BidderError::FailedToUnmarshal(e.to_string()))
}

/// Go `adapters.ExtImpBidder` (only the part adapters read).
#[allow(dead_code)]
#[derive(Debug, Default, serde::Deserialize)]
struct ExtImpBidder {
    #[serde(default)]
    bidder: Option<Ext>,
}

/// `imp.ext` -> `ext.bidder` -> `T`, the usual two-step decode.
#[allow(dead_code)]
fn decode_bidder<T: serde::de::DeserializeOwned>(imp: &Imp) -> Result<T, BidderError> {
    let bidder_ext: ExtImpBidder = decode_ext(imp.ext.as_ref())?;
    decode_ext(bidder_ext.bidder.as_ref())
}

#[allow(dead_code)]
fn ext_from<T: serde::Serialize>(v: &T) -> Result<Ext, BidderError> {
    let bytes = crate::go_json::to_vec(v).map_err(|e| BidderError::other(e.to_string()))?;
    Ext::from_slice(&bytes).map_err(|e| BidderError::other(e.to_string()))
}

#[allow(dead_code)]
fn marshal<T: serde::Serialize>(v: &T) -> Result<Vec<u8>, BidderError> {
    crate::go_json::to_vec(v).map_err(|e| BidderError::other(e.to_string()))
}

#[allow(dead_code)]
fn imp_ids(imps: &[Imp]) -> Vec<String> {
    imps.iter().map(|i| i.id.clone()).collect()
}

/// Go `json.Number`: accepts a JSON number or string, keeps the text.
#[allow(dead_code)]
#[derive(Debug, Default, Clone, PartialEq)]
struct JsonNumber(String);

impl<'de> serde::Deserialize<'de> for JsonNumber {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = JsonNumber;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a number or string")
            }
            fn visit_i64<E>(self, v: i64) -> Result<JsonNumber, E> {
                Ok(JsonNumber(v.to_string()))
            }
            fn visit_u64<E>(self, v: u64) -> Result<JsonNumber, E> {
                Ok(JsonNumber(v.to_string()))
            }
            fn visit_f64<E>(self, v: f64) -> Result<JsonNumber, E> {
                Ok(JsonNumber(v.to_string()))
            }
            fn visit_str<E>(self, v: &str) -> Result<JsonNumber, E> {
                Ok(JsonNumber(v.to_string()))
            }
            fn visit_string<E>(self, v: String) -> Result<JsonNumber, E> {
                Ok(JsonNumber(v))
            }
            fn visit_unit<E>(self) -> Result<JsonNumber, E> {
                Ok(JsonNumber(String::new()))
            }
            fn visit_none<E>(self) -> Result<JsonNumber, E> {
                Ok(JsonNumber(String::new()))
            }
        }
        d.deserialize_any(V)
    }
}

#[allow(dead_code)]
impl JsonNumber {
    /// Go `Number.String`.
    fn as_str(&self) -> &str {
        &self.0
    }
    /// Go `Number.Int64` (`strconv.ParseInt(s, 10, 64)`).
    fn int64(&self) -> Result<i64, String> {
        self.0.parse::<i64>().map_err(|e| {
            use std::num::IntErrorKind::*;
            let why = match e.kind() {
                PosOverflow | NegOverflow => "value out of range",
                _ => "invalid syntax",
            };
            format!("strconv.ParseInt: parsing {:?}: {why}", self.0)
        })
    }
    /// Go `Number.Float64`.
    fn float64(&self) -> Result<f64, String> {
        self.0
            .parse::<f64>()
            .map_err(|_| format!("strconv.ParseFloat: parsing {:?}: invalid syntax", self.0))
    }
}

/// Go `template.New("endpointTemplate").Parse(endpoint)`. Go's parser rejects a call to an
/// undefined function (`{{Malformed}}`) at parse time, so a bare identifier fails here too.
#[allow(dead_code)]
fn build_template(endpoint: &str) -> Result<crate::macros::EndpointTemplate, BidderError> {
    let fail = |e: String| BidderError::other(format!("unable to parse endpoint url template: {e}"));
    let t = crate::macros::EndpointTemplate::parse(endpoint).map_err(fail)?;
    if let Err(m) = t.resolve(&crate::macros::EndpointTemplateParams::default()) {
        if !m.contains("function \".") {
            return Err(fail(m));
        }
    }
    Ok(t)
}

#[allow(dead_code)]
fn status_err(code: u16, suffix: &str) -> String {
    format!("Unexpected status code: {code}.{suffix}")
}

/// jsoniter's wording for a JSON string field that holds another JSON type, as
/// `jsonutil.Unmarshal` reports it (`cannot unmarshal {struct}.{Field}: expects " or n, but found X`).
/// serde's message carries neither the struct path nor the offending byte, so the string fields
/// are checked up front; the first mismatch wins.
#[allow(dead_code)]
fn check_string_fields(
    ext: Option<&Ext>,
    go_struct: &str,
    fields: &[(&str, &str)],
) -> Result<(), BidderError> {
    use sonic_rs::JsonValueTrait;
    let Some(ext) = ext else { return Ok(()) };
    if !ext.0.is_object() {
        return Ok(());
    }
    for (key, go_field) in fields {
        if let Some(v) = ext.0.get(*key) {
            if !v.is_str() && !v.is_null() {
                let found = v.to_string().chars().next().unwrap_or('\u{0}');
                return Err(BidderError::FailedToUnmarshal(format!(
                    "cannot unmarshal {go_struct}.{go_field}: expects \" or n, but found {found}"
                )));
            }
        }
    }
    Ok(())
}


/// Go `validateImpression`: returns the source id after rewriting the imp.
fn validate_impression(imp: &mut Imp) -> Result<i64, BidderError> {
    if imp.banner.is_none() && imp.video.is_none() {
        return Err(BidderError::bad_input(format!(
            "ignoring imp id={}, Adtelligent supports only Video and Banner",
            imp.id
        )));
    }
    if imp.ext.is_none() {
        return Err(BidderError::bad_input(format!("ignoring imp id={}, extImpBidder is empty", imp.id)));
    }
    let bidder_ext: ExtImpBidder = decode_ext(imp.ext.as_ref()).map_err(|e| {
        BidderError::bad_input(format!("ignoring imp id={}, error while decoding extImpBidder, err: {}", imp.id, e))
    })?;
    let imp_ext: ExtImpAdtelligent = decode_ext(bidder_ext.bidder.as_ref()).map_err(|e| {
        BidderError::bad_input(format!("ignoring imp id={}, error while decoding impExt, err: {}", imp.id, e))
    })?;
    let buffer = serde_json::to_vec(&AdtelligentImpExt { adtelligent: &imp_ext }).map_err(|e| {
        BidderError::bad_input(format!("ignoring imp id={}, error while marshaling impExt, err: {}", imp.id, e))
    })?;
    if imp_ext.bid_floor > 0.0 {
        imp.bidfloor = imp_ext.bid_floor;
    }
    imp.ext = Some(Ext::from_slice(&buffer).map_err(|e| BidderError::bad_input(e.to_string()))?);
    let aid = imp_ext
        .source_id
        .0
        .int64()
        .map_err(|e| BidderError::bad_input(format!("ignoring imp id={}, aid parsing err: {}", imp.id, e)))?;
    Ok(aid)
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errors = vec![];
        // Go iterates a map (random order); source ids are kept in first-seen order here.
        let mut groups: Vec<(i64, Vec<Imp>)> = vec![];
        for imp in &request.imp {
            let mut imp = imp.clone();
            match validate_impression(&mut imp) {
                Err(e) => errors.push(e),
                Ok(source_id) => match groups.iter_mut().find(|(id, _)| *id == source_id) {
                    Some((_, imps)) => imps.push(imp),
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

        let mut reqs = Vec::with_capacity(groups.len());
        for (source_id, imps) in groups {
            let mut req = request.clone();
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
        request: &BidRequest,
        _request_data: &RequestData,
        response_data: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if response_data.status_code == 204 {
            return (None, vec![]);
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(e) => {
                return (
                    None,
                    vec![BidderError::bad_server_response(format!("error while decoding response, err: {e}"))],
                )
            }
        };
        let mut out = BidderResponse::new();
        let mut errors = vec![];
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let mut imp_ok = false;
                let mut media_type = BidType::Banner;
                for imp in &request.imp {
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
                out.bids.push(TypedBid::new(bid, media_type));
            }
        }
        (Some(out), errors)
    }
}
