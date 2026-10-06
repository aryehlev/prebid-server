//! Go `adapters/rubicon/rubicon.go`.
//!
//! Go works on `map[string]json.RawMessage` / `map[string]interface{}` for the first-party-data
//! targets and writes them with `json.Marshal`, which sorts keys; `BTreeMap` does the same here.

#![allow(clippy::too_many_arguments)]

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::value::RawValue;
use serde_json::Value;

use crate::bid_types::{BidType, ExtBidPrebidMeta};
use crate::bidder::{Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use crate::config;
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{
    Banner, Bid, BidRequest, Data, Device, Eid, Imp, Native, Publisher, SupplyChain, User, Video,
};
use crate::ortb::Ext;

const BADV_LIMIT_SIZE: usize = 50;

type Map = BTreeMap<String, Value>;

pub struct Adapter {
    uri: String,
    external_uri: String,
    xapi_username: String,
    xapi_password: String,
    /// Go `version.Ver`, set from the build; empty when unset (as in Go's tests).
    pbs_version: String,
}

// ---------------------------------------------------------------------------------------------
// Request-side ext types (Go `rubicon*` structs)
// ---------------------------------------------------------------------------------------------

/// Go `json.Number`: a JSON number or a string, kept as text.
#[derive(Debug, Default, Clone)]
struct Number(String);

impl<'de> Deserialize<'de> for Number {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        match Value::deserialize(d)? {
            Value::Number(n) => Ok(Number(n.to_string())),
            Value::String(s) => Ok(Number(s)),
            Value::Null => Ok(Number(String::new())),
            other => Err(serde::de::Error::custom(format!("cannot unmarshal {other} into json.Number"))),
        }
    }
}

impl Number {
    /// `json.Number.Int64()`.
    fn int64(&self) -> Result<i64, BidderError> {
        self.0.parse::<i64>().map_err(|_| {
            let digits = self.0.strip_prefix(['+', '-']).unwrap_or(&self.0);
            let reason = if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
                "value out of range"
            } else {
                "invalid syntax"
            };
            BidderError::other(format!("strconv.ParseInt: parsing \"{}\": {reason}", self.0))
        })
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpRubicon {
    #[serde(rename = "accountId")]
    account_id: Number,
    #[serde(rename = "siteId")]
    site_id: Number,
    #[serde(rename = "zoneId")]
    zone_id: Number,
    inventory: Option<Box<RawValue>>,
    bidonmultiformat: bool,
    keywords: Vec<String>,
    visitor: Option<Box<RawValue>>,
    video: RubiconVideoParams,
    debug: ImpExtRubiconDebug,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RubiconVideoParams {
    #[serde(rename = "size_id")]
    video_size_id: i64,
    skip: i64,
    skipdelay: i64,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ImpExtRubiconDebug {
    cpmoverride: f64,
}

/// Go `rubiconExtImpBidder` with the bidder params already decoded.
struct RubiconExtImpBidder {
    bidder: ExtImpRubicon,
    gpid: String,
    skadn: Option<Box<RawValue>>,
    tid: String,
    data: Option<Box<RawValue>>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawExtImpBidder {
    bidder: Option<Box<RawValue>>,
    gpid: String,
    skadn: Option<Box<RawValue>>,
    tid: String,
    data: Option<Box<RawValue>>,
}

#[derive(Serialize)]
struct RubiconImpExt {
    rp: RubiconImpExtRp,
    #[serde(skip_serializing_if = "String::is_empty")]
    gpid: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    skadn: Option<Box<RawValue>>,
    #[serde(skip_serializing_if = "String::is_empty")]
    tid: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    maxbids: Option<i64>,
}

#[derive(Serialize)]
struct RubiconImpExtRp {
    zone_id: i64,
    target: Value,
    track: RubiconImpExtRpTrack,
}

#[derive(Serialize)]
struct RubiconImpExtRpTrack {
    mint: String,
    mint_version: String,
}

#[derive(Serialize)]
struct RubiconUserExt {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    eids: Vec<Eid>,
    rp: RubiconUserExtRp,
    #[serde(skip_serializing_if = "String::is_empty")]
    consent: String,
}

#[derive(Serialize)]
struct RubiconUserExtRp {
    target: Value,
}

#[derive(Serialize)]
struct RubiconSiteExt {
    rp: RubiconSiteExtRp,
}

#[derive(Serialize)]
struct RubiconSiteExtRp {
    site_id: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    target: Option<Value>,
}

#[derive(Serialize)]
struct RubiconPubExt {
    rp: RubiconPubExtRp,
}

#[derive(Serialize)]
struct RubiconPubExtRp {
    account_id: i64,
}

#[derive(Serialize)]
struct RubiconVideoExt {
    #[serde(skip_serializing_if = "is_zero")]
    skip: i64,
    #[serde(skip_serializing_if = "is_zero")]
    skipdelay: i64,
    #[serde(skip_serializing_if = "String::is_empty")]
    videotype: String,
    rp: RubiconVideoExtRp,
}

#[derive(Serialize)]
struct RubiconVideoExtRp {
    #[serde(skip_serializing_if = "is_zero")]
    size_id: i64,
}

#[derive(Serialize)]
struct RubiconDeviceExt {
    rp: RubiconDeviceExtRp,
}

#[derive(Serialize)]
struct RubiconDeviceExtRp {
    pixelratio: f64,
}

fn is_zero(v: &i64) -> bool {
    *v == 0
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RubiconData {
    adserver: RubiconAdServer,
    pbadslot: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RubiconAdServer {
    name: String,
    adslot: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RubiconDataExt {
    segtax: i64,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct BidRequestExt {
    prebid: BidRequestExtPrebid,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct BidRequestExtPrebid {
    bidders: BidRequestExtPrebidBidders,
    multibid: Vec<Option<ExtMultiBid>>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct BidRequestExtPrebidBidders {
    rubicon: PrebidBiddersRubicon,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct PrebidBiddersRubicon {
    debug: PrebidBiddersRubiconDebug,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct PrebidBiddersRubiconDebug {
    cpmoverride: f64,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtMultiBid {
    maxbids: Option<i64>,
}

/// Go `openrtb_ext.ExtSource`.
#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct ExtSource {
    schain: Option<SupplyChain>,
}

/// Go `openrtb_ext.ExtRegs`.
#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct ExtRegs {
    #[serde(skip_serializing_if = "Option::is_none")]
    dsa: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    gdpr: Option<i8>,
    #[serde(skip_serializing_if = "String::is_empty")]
    us_privacy: String,
}

// ---------------------------------------------------------------------------------------------
// Response-side types
// ---------------------------------------------------------------------------------------------

#[derive(Deserialize, Default)]
#[serde(default)]
struct RubiconBidResponse {
    bidid: String,
    cur: String,
    seatbid: Vec<RubiconSeatBid>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RubiconSeatBid {
    buyer: String,
    seat: String,
    bid: Vec<RubiconBid>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RubiconBid {
    #[serde(flatten)]
    bid: Bid,
    adm_native: Option<Value>,
}

/// Go `openrtb_ext.ExtBidPrebid`, kept in full because `updateBidExtWithMeta` re-marshals it.
#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct ExtBidPrebid {
    #[serde(skip_serializing_if = "Option::is_none")]
    cache: Option<ExtBidPrebidCache>,
    #[serde(skip_serializing_if = "is_zero")]
    dealpriority: i64,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    dealtiersatisfied: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    meta: Option<ExtBidPrebidMeta>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    targeting: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    targetbiddercode: String,
    #[serde(rename = "type", skip_serializing_if = "String::is_empty")]
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    video: Option<crate::bid_types::ExtBidPrebidVideo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    events: Option<ExtBidPrebidEvents>,
    #[serde(skip_serializing_if = "String::is_empty")]
    bidid: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    passthrough: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    floors: Option<ExtBidPrebidFloors>,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct ExtBidPrebidCache {
    key: String,
    url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    bids: Option<ExtBidPrebidCacheBids>,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct ExtBidPrebidCacheBids {
    url: String,
    #[serde(rename = "cacheId")]
    cache_id: String,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct ExtBidPrebidEvents {
    #[serde(skip_serializing_if = "String::is_empty")]
    win: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    imp: String,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct ExtBidPrebidFloors {
    #[serde(rename = "floorRule", skip_serializing_if = "String::is_empty")]
    floor_rule: String,
    #[serde(rename = "floorRuleValue", skip_serializing_if = "is_zero_f")]
    floor_rule_value: f64,
    #[serde(rename = "floorValue", skip_serializing_if = "is_zero_f")]
    floor_value: f64,
    #[serde(rename = "floorCurrency", skip_serializing_if = "String::is_empty")]
    floor_currency: String,
}

fn is_zero_f(v: &f64) -> bool {
    *v == 0.0
}

/// Go `extPrebid`.
#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct ExtPrebid {
    #[serde(skip_serializing_if = "Option::is_none")]
    prebid: Option<ExtBidPrebid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bidder: Option<Value>,
}

// ---------------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------------

fn ext_text(ext: &Option<Ext>) -> Vec<u8> {
    ext.as_ref().map(|e| e.to_json().into_bytes()).unwrap_or_default()
}

/// `jsonutil.Unmarshal` of a nil `json.RawMessage`: jsoniter reports a NUL byte.
fn unmarshal_raw<T: serde::de::DeserializeOwned>(data: &[u8]) -> Result<T, BidderError> {
    if data.iter().all(|b| b" \t\r\n".contains(b)) {
        return Err(BidderError::FailedToUnmarshal("expect { or n, but found \u{0}".to_string()));
    }
    jsonutil::unmarshal(data)
}

fn to_ext<T: Serialize>(v: &T) -> Result<Ext, BidderError> {
    let bytes = crate::go_json::to_vec(v).map_err(|e| BidderError::other(e.to_string()))?;
    Ext::from_slice(&bytes).map_err(|e| BidderError::other(e.to_string()))
}

/// Parses an ext into a key-sorted map (Go `map[string]json.RawMessage`).
fn parse_ext_map(ext: &Option<Ext>) -> Result<Map, BidderError> {
    match ext {
        None => Ok(Map::new()),
        Some(e) => jsonutil::unmarshal(e.to_json().as_bytes()),
    }
}

/// Writes the map back; an empty map is a nil ext (Go `marshal()` returns nil).
fn write_ext_map(map: &Map) -> Result<Option<Ext>, BidderError> {
    if map.is_empty() {
        return Ok(None);
    }
    to_ext(map).map(Some)
}

/// Go `appendTrackerToUrl`: `url.Parse`, `Query().Add`, `Encode()` (keys sorted), `String()`.
fn append_tracker_to_url(uri: &str, tracker: &str) -> String {
    let (rest, fragment) = match uri.split_once('#') {
        Some((r, f)) => (r, Some(f)),
        None => (uri, None),
    };
    let (base, query) = match rest.split_once('?') {
        Some((b, q)) => (b, q),
        None => (rest, ""),
    };
    let mut values: Vec<(String, String)> = Vec::new();
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        // Go drops pairs that fail to unescape.
        if let (Some(k), Some(v)) = (query_unescape(k), query_unescape(v)) {
            values.push((k, v));
        }
    }
    values.push(("tk_xint".to_string(), tracker.to_string()));
    values.sort_by(|a, b| a.0.cmp(&b.0)); // stable: values of one key keep their order
    let encoded: Vec<String> =
        values.iter().map(|(k, v)| format!("{}={}", query_escape(k), query_escape(v))).collect();
    let mut out = format!("{base}?{}", encoded.join("&"));
    if let Some(f) = fragment {
        out.push('#');
        out.push_str(f);
    }
    out
}

fn query_escape(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn query_unescape(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                let hex = s.get(i + 1..i + 3)?;
                out.push(u8::from_str_radix(hex, 16).ok()?);
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(cfg: &config::Adapter, server: &config::Server) -> Self {
        Self {
            uri: append_tracker_to_url(&cfg.endpoint, &cfg.xapi.tracker),
            external_uri: server.external_url.clone(),
            xapi_username: cfg.xapi.username.clone(),
            xapi_password: cfg.xapi.password.clone(),
            pbs_version: String::new(),
        }
    }

    /// Sets what Go reads from `version.Ver` (`pbs_version` in the imp target).
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.pbs_version = version.into();
        self
    }
}

// ---------------------------------------------------------------------------------------------
// OpenRTB 2.5 -> 2.6 upgrade (Go `openrtb_ext.ConvertUpTo26` + `RebuildRequest`)
// ---------------------------------------------------------------------------------------------

fn ext_invalid(what: &str, e: BidderError) -> BidderError {
    BidderError::other(format!("{what} is invalid: {e}"))
}

fn schain_from(map: &Map) -> Result<Option<SupplyChain>, BidderError> {
    match map.get("schain") {
        None | Some(Value::Null) => Ok(None),
        Some(v) => serde_json::from_value(v.clone())
            .map(Some)
            .map_err(|e| BidderError::FailedToUnmarshal(e.to_string())),
    }
}

fn update_request_to_26(r: &mut BidRequest) -> Result<(), BidderError> {
    // convertUpEnsureExt: every ext involved must parse before anything moves.
    let mut req_ext = parse_ext_map(&r.ext).map_err(|e| ext_invalid("req.ext", e))?;
    let schain24 = schain_from(&req_ext).map_err(|e| ext_invalid("req.ext", e))?;
    let mut source_ext =
        parse_ext_map(&r.source.as_ref().and_then(|s| s.ext.clone())).map_err(|e| ext_invalid("req.source.ext", e))?;
    let schain25 = schain_from(&source_ext).map_err(|e| ext_invalid("req.source.ext", e))?;
    let mut regs_ext =
        parse_ext_map(&r.regs.as_ref().and_then(|s| s.ext.clone())).map_err(|e| ext_invalid("req.regs.ext", e))?;
    let gdpr25: Option<i8> = match regs_ext.get("gdpr") {
        None | Some(Value::Null) => None,
        Some(v) => Some(
            serde_json::from_value(v.clone()).map_err(|_| ext_invalid("req.regs.ext", BidderError::other("gdpr must be an integer")))?,
        ),
    };
    let usp25: String = match regs_ext.get("us_privacy") {
        None | Some(Value::Null) => String::new(),
        Some(v) => serde_json::from_value(v.clone())
            .map_err(|e| ext_invalid("req.regs.ext", BidderError::FailedToUnmarshal(e.to_string())))?,
    };
    let mut user_ext =
        parse_ext_map(&r.user.as_ref().and_then(|s| s.ext.clone())).map_err(|e| ext_invalid("req.user.ext", e))?;
    let consent25: Option<String> = match user_ext.get("consent") {
        None | Some(Value::Null) => None,
        Some(v) => Some(
            serde_json::from_value(v.clone())
                .map_err(|e| ext_invalid("req.user.ext", BidderError::FailedToUnmarshal(e.to_string())))?,
        ),
    };
    let eid25: Option<Vec<Eid>> = match user_ext.get("eids") {
        None | Some(Value::Null) => None,
        Some(v) => Some(
            serde_json::from_value(v.clone())
                .map_err(|e| ext_invalid("req.user.ext", BidderError::FailedToUnmarshal(e.to_string())))?,
        ),
    };
    let mut imp_exts = Vec::with_capacity(r.imp.len());
    for (i, imp) in r.imp.iter().enumerate() {
        let m = parse_ext_map(&imp.ext).map_err(|e| ext_invalid(&format!("imp[{i}].imp.ext"), e))?;
        imp_exts.push(m);
    }

    // schain 2.4 -> 2.5 -> 2.6
    req_ext.remove("schain");
    let schain = schain25.or(schain24);
    source_ext.remove("schain");
    if let Some(schain) = schain {
        let source = r.source.get_or_insert_with(Default::default);
        if source.schain.is_none() {
            source.schain = Some(schain);
        }
    }
    r.ext = write_ext_map(&req_ext)?;
    if let Some(source) = r.source.as_mut() {
        source.ext = write_ext_map(&source_ext)?;
    }

    // gdpr and us_privacy
    regs_ext.remove("gdpr");
    regs_ext.remove("us_privacy");
    if let Some(regs) = r.regs.as_mut() {
        if let Some(g) = gdpr25 {
            if regs.gdpr.is_none() {
                regs.gdpr = Some(g);
            }
        }
        if !usp25.is_empty() && regs.us_privacy.is_empty() {
            regs.us_privacy = usp25;
        }
        regs.ext = write_ext_map(&regs_ext)?;
    }

    // consent and eids
    user_ext.remove("consent");
    user_ext.remove("eids");
    if let Some(user) = r.user.as_mut() {
        if let Some(c) = consent25 {
            if user.consent.is_empty() {
                user.consent = c;
            }
        }
        if let Some(e) = eid25 {
            if user.eids.is_empty() {
                user.eids = e;
            }
        }
        user.ext = write_ext_map(&user_ext)?;
    }

    // imp: prebid.is_rewarded_inventory -> imp.rwdd
    for (imp, mut ext) in r.imp.iter_mut().zip(imp_exts) {
        let Some(Value::Object(prebid)) = ext.get_mut("prebid") else { continue };
        let Some(rewarded) = prebid.get("is_rewarded_inventory").and_then(Value::as_i64) else { continue };
        prebid.remove("is_rewarded_inventory");
        if prebid.is_empty() {
            ext.remove("prebid");
        }
        if imp.rwdd == 0 {
            imp.rwdd = rewarded as i8;
        }
        imp.ext = write_ext_map(&ext)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// First party data
// ---------------------------------------------------------------------------------------------

/// jsonparser.Get(ext, path...): the raw value at `path`, `None` when the path is missing.
fn get_ext_path(ext: &Option<Ext>, path: &[&str]) -> Result<Option<Vec<u8>>, BidderError> {
    let Some(e) = ext else { return Ok(None) };
    let mut v: Value = jsonutil::unmarshal(e.to_json().as_bytes())?;
    for key in path {
        match v {
            Value::Object(mut m) => match m.remove(*key) {
                Some(next) => v = next,
                None => return Ok(None),
            },
            _ => return Ok(None),
        }
    }
    Ok(Some(serde_json::to_vec(&v).map_err(|e| BidderError::other(e.to_string()))?))
}

/// Go `rawJSONToMap`: nil is an empty map. A JSON `null` is treated as empty too (Go would hold
/// a nil map and panic on the next write).
fn raw_json_to_map(raw: Option<&[u8]>) -> Result<Map, BidderError> {
    match raw {
        None => Ok(Map::new()),
        Some(b) if b == b"null" => Ok(Map::new()),
        Some(b) => jsonutil::unmarshal(b),
    }
}

fn str_array(s: String) -> Value {
    Value::Array(vec![Value::String(s)])
}

/// Go `populateFirstPartyDataAttributes`.
fn populate_first_party_data_attributes(source: Option<&[u8]>, target: &mut Map) -> Result<(), BidderError> {
    let source_map = raw_json_to_map(source)?;
    for (key, val) in source_map {
        match val {
            Value::String(s) => {
                target.insert(key, str_array(s));
            }
            Value::Number(n) => {
                if let Some(f) = n.as_f64() {
                    if f == (f as i64) as f64 {
                        target.insert(key, str_array((f as i64).to_string()));
                    }
                }
            }
            Value::Bool(b) => {
                target.insert(key, str_array(b.to_string()));
            }
            Value::Array(arr) => {
                let all_strings = arr.iter().all(Value::is_string);
                let all_bools = arr.iter().all(Value::is_boolean);
                if all_strings {
                    target.insert(key.clone(), Value::Array(arr.clone()));
                }
                if all_bools {
                    // An empty array is both: Go's `convertToStringArray` returns a nil slice, so
                    // the key ends up `null`.
                    if arr.is_empty() {
                        target.insert(key, Value::Null);
                    } else {
                        let strs = arr
                            .iter()
                            .filter_map(Value::as_bool)
                            .map(|b| Value::String(b.to_string()))
                            .collect();
                        target.insert(key, Value::Array(strs));
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn get_segment_ids_to_copy(data: &[Data], seg_taxes: &[i64]) -> Vec<String> {
    let mut ids = Vec::new();
    for record in data {
        let Some(ext) = &record.ext else { continue };
        let Ok(ext_obj) = jsonutil::unmarshal::<RubiconDataExt>(ext.to_json().as_bytes()) else { continue };
        if seg_taxes.contains(&ext_obj.segtax) {
            ids.extend(record.segment.iter().map(|s| s.id.clone()));
        }
    }
    ids
}

fn update_ext_with_iab_attribute(target: &mut Map, data: &[Data], seg_taxes: &[i64]) {
    let ids = get_segment_ids_to_copy(data, seg_taxes);
    if ids.is_empty() {
        return;
    }
    target.insert("iab".to_string(), Value::Array(ids.into_iter().map(Value::String).collect()));
}

fn update_user_rp_target_with_fpd_attributes(visitor: Option<&[u8]>, user: &User) -> Result<Value, BidderError> {
    let existing = get_ext_path(&user.ext, &["rp", "target"])?;
    let mut target = raw_json_to_map(existing.as_deref())?;
    populate_first_party_data_attributes(visitor, &mut target)?;
    let user_ext_data = get_ext_path(&user.ext, &["data"])?;
    populate_first_party_data_attributes(user_ext_data.as_deref(), &mut target)?;
    update_ext_with_iab_attribute(&mut target, &user.data, &[4]);
    Ok(Value::Object(target.into_iter().collect()))
}

fn is_video(imp: &Imp) -> bool {
    match &imp.video {
        Some(video) => imp.banner.is_none() || is_fully_populated_video(video),
        None => false,
    }
}

/// Go checks `MIMEs != nil && Protocols != nil`. `mimes` keeps nil apart from `[]`; `protocols`
/// is a plain `Vec`, so an empty-but-present array counts as missing there.
fn is_fully_populated_video(video: &Video) -> bool {
    video.mimes.is_some()
        && !video.protocols.is_empty()
        && video.maxduration != 0
        && video.linearity.0 != 0
}

fn resolve_native_object(native: Option<&Native>, target: &mut Map) -> Result<(), BidderError> {
    let Some(native) = native else {
        return Err(BidderError::other("Native object is not present for request"));
    };
    if native.ver == "1.0" || native.ver == "1.1" {
        return Ok(());
    }
    let parsed: Map = jsonutil::unmarshal(native.request.as_bytes())?;
    target.extend(parsed);

    if !matches!(target.get("eventtrackers"), Some(Value::Array(_))) {
        return Err(BidderError::other("Eventtrackers are not present or not of array type"));
    }
    match target.get("context") {
        None | Some(Value::Null) => {}
        Some(Value::Number(_)) => {}
        Some(_) => return Err(BidderError::other("Context is not of int type")),
    }
    if !matches!(target.get("plcmttype"), Some(Value::Number(_))) {
        return Err(BidderError::other("Plcmttype is not present or not of int type"));
    }
    Ok(())
}

/// Go `setImpNative`: adds `request_native` to `imp[0].native` of the serialized request.
fn set_imp_native(json_data: &[u8], request_native: &Map) -> Result<Vec<u8>, BidderError> {
    let mut json_map: Map = jsonutil::unmarshal(json_data)?;
    let imp0 = match json_map.get_mut("imp") {
        Some(Value::Array(a)) => match a.first_mut() {
            Some(Value::Object(o)) => o,
            Some(_) => return Err(BidderError::other("unexpected type for imp[0] found in json data")),
            None => return Err(BidderError::other("unable to find imp[0] in json data")),
        },
        _ => return Err(BidderError::other("unable to find imp in json data")),
    };
    let Some(Value::Object(native)) = imp0.get_mut("native") else {
        return Err(BidderError::other("unable to find imp[0].native in json data"));
    };
    native.insert(
        "request_native".to_string(),
        Value::Object(request_native.iter().map(|(k, v)| (k.clone(), v.clone())).collect()),
    );
    crate::go_json::to_vec(&json_map)
        .map_err(|e| BidderError::other(format!("unable to encode json data ({e})")))
}

fn split_multi_format_imp(imp: &Imp) -> Vec<Imp> {
    let mut out = Vec::new();
    if imp.banner.is_some() {
        let mut c = imp.clone();
        c.video = None;
        c.native = None;
        c.audio = None;
        out.push(c);
    }
    if imp.video.is_some() {
        let mut c = imp.clone();
        c.banner = None;
        c.native = None;
        c.audio = None;
        out.push(c);
    }
    if imp.native.is_some() {
        let mut c = imp.clone();
        c.banner = None;
        c.video = None;
        c.audio = None;
        out.push(c);
    }
    if imp.audio.is_some() {
        let mut c = imp.clone();
        c.banner = None;
        c.video = None;
        c.native = None;
        out.push(c);
    }
    out
}

/// Go `createImpsToExtMap` + `prepareImpsToExtMap` (Go iterates a map, so order is arbitrary;
/// request order is kept here).
fn create_imps_to_ext(imps: &[Imp]) -> (Vec<(Imp, RubiconExtImpBidder)>, Vec<BidderError>) {
    let mut out = Vec::new();
    let mut errs = Vec::new();
    for imp in imps {
        let ext = match parse_rubicon_ext_imp_bidder(imp) {
            Ok(e) => e,
            Err(e) => {
                errs.push(BidderError::bad_input(e.to_string()));
                continue;
            }
        };
        if !ext.bidder.bidonmultiformat {
            out.push((imp.clone(), ext));
            continue;
        }
        for split in split_multi_format_imp(imp) {
            let ext = parse_rubicon_ext_imp_bidder(imp).expect("parsed above");
            out.push((split, ext));
        }
    }
    (out, errs)
}

fn parse_rubicon_ext_imp_bidder(imp: &Imp) -> Result<RubiconExtImpBidder, BidderError> {
    let raw: RawExtImpBidder = unmarshal_raw(&ext_text(&imp.ext))?;
    let bidder: ExtImpRubicon = match &raw.bidder {
        None => ExtImpRubicon::default(),
        Some(b) => {
            let text = b.get();
            let first = text.trim_start().as_bytes().first().copied().unwrap_or(b'n');
            if first != b'{' && first != b'n' {
                return Err(BidderError::FailedToUnmarshal(format!(
                    "cannot unmarshal rubicon.rubiconExtImpBidder.Bidder: expect {{ or n, but found {}",
                    first as char
                )));
            }
            jsonutil::unmarshal(text.as_bytes())?
        }
    };
    Ok(RubiconExtImpBidder { bidder, gpid: raw.gpid, skadn: raw.skadn, tid: raw.tid, data: raw.data })
}

fn add_string_attribute(attribute: &str, target: &mut Map, name: &str) {
    target.insert(name.to_string(), str_array(attribute.to_string()));
}

impl Adapter {
    /// Go `updateImpRpTarget`.
    fn update_imp_rp_target(
        &self,
        ext_imp: &RubiconExtImpBidder,
        ext_imp_rubicon: &ExtImpRubicon,
        imp: &Imp,
        site: Option<&crate::ortb::openrtb2::Site>,
        app: Option<&crate::ortb::openrtb2::App>,
    ) -> Result<Value, BidderError> {
        let existing = get_ext_path(&imp.ext, &["rp", "target"])?;
        let mut target = raw_json_to_map(existing.as_deref())?;
        populate_first_party_data_attributes(ext_imp_rubicon.inventory.as_ref().map(|r| r.get().as_bytes()), &mut target)?;

        if let Some(site) = site {
            let data = get_ext_path(&site.ext, &["data"])?;
            populate_first_party_data_attributes(data.as_deref(), &mut target)?;
            if !site.page.is_empty() {
                add_string_attribute(&site.page, &mut target, "page");
            }
        } else {
            // Go dereferences `app` here and panics when it is nil too; report it instead.
            let app = app.ok_or_else(|| BidderError::bad_input("request has neither site nor app"))?;
            let data = get_ext_path(&app.ext, &["data"])?;
            populate_first_party_data_attributes(data.as_deref(), &mut target)?;
        }

        let imp_data = ext_imp.data.as_ref().map(|r| r.get().as_bytes());
        if let Some(d) = imp_data {
            populate_first_party_data_attributes(Some(d), &mut target)?;
        }

        let data: RubiconData = match imp_data {
            Some(d) => jsonutil::unmarshal(d)?,
            None => RubiconData::default(),
        };
        if !data.pbadslot.is_empty() {
            target.insert("pbadslot".to_string(), Value::String(data.pbadslot));
        } else if data.adserver.name == "gam" && !data.adserver.adslot.is_empty() {
            target.insert("dfp_ad_unit_code".to_string(), Value::String(data.adserver.adslot));
        }

        if !ext_imp_rubicon.keywords.is_empty() {
            target.insert(
                "keywords".to_string(),
                Value::Array(ext_imp_rubicon.keywords.iter().cloned().map(Value::String).collect()),
            );
        }

        target.insert("pbs_login".to_string(), Value::String(self.xapi_username.clone()));
        target.insert("pbs_version".to_string(), Value::String(self.pbs_version.clone()));
        target.insert("pbs_url".to_string(), Value::String(self.external_uri.clone()));

        Ok(Value::Object(target.into_iter().collect()))
    }
}

fn get_max_bids(request: &BidRequest) -> Option<i64> {
    let ext: BidRequestExt = jsonutil::unmarshal(&ext_text(&request.ext)).ok()?;
    ext.prebid.multibid.into_iter().next().flatten()?.maxbids
}

fn resolve_bid_floor(
    bid_floor: f64,
    bid_floor_cur: &str,
    req_info: &ExtraRequestInfo,
) -> Result<f64, BidderError> {
    if bid_floor > 0.0 && !bid_floor_cur.is_empty() && bid_floor_cur.to_uppercase() != "USD" {
        return req_info.convert_currency(bid_floor, bid_floor_cur, "USD");
    }
    Ok(bid_floor)
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut request = request.clone();
        if let Err(e) = update_request_to_26(&mut request) {
            return (vec![], vec![e]);
        }

        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        headers.add("User-Agent", "prebid-server/1.0");

        let (imps_to_ext, mut errs) = create_imps_to_ext(&request.imp);
        let max_bids = get_max_bids(&request);

        let mut request_data = Vec::with_capacity(imps_to_ext.len());
        for (mut imp, bidder_ext) in imps_to_ext {
            match self.build_request(&request, &mut imp, &bidder_ext, max_bids, req_info) {
                Ok(body_and_ids) => {
                    let (body, imp_ids) = body_and_ids;
                    let mut data = RequestData {
                        method: "POST".into(),
                        uri: self.uri.clone(),
                        body,
                        headers: headers.clone(),
                        imp_ids,
                    };
                    data.set_basic_auth(&self.xapi_username, &self.xapi_password);
                    request_data.push(data);
                }
                Err(e) => errs.push(e),
            }
        }
        (request_data, errs)
    }

    fn make_bids(
        &self,
        internal_request: &BidRequest,
        external_request: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        let code = response.status_code;
        if code == 204 {
            return (None, vec![]);
        }
        if code == 400 {
            return (
                None,
                vec![BidderError::bad_input(format!(
                    "Unexpected status code: {code}. Run with request.debug = 1 for more info"
                ))],
            );
        }
        if code != 200 {
            return (
                None,
                vec![BidderError::bad_server_response(format!(
                    "Unexpected status code: {code}. Run with request.debug = 1 for more info"
                ))],
            );
        }

        let bid_resp: RubiconBidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![BidderError::bad_server_response(e.to_string())]),
        };

        let bid_req: BidRequest = match jsonutil::unmarshal(&external_request.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        // Go indexes `bidReq.Imp[0]` and panics on an empty imp list; return an error instead.
        let Some(first_imp) = bid_req.imp.first() else {
            return (None, vec![BidderError::bad_input("no impressions in the bidder request")]);
        };

        let mut out = BidderResponse::with_bids_capacity(5);
        let bid_type = if is_video(first_imp) {
            BidType::Video
        } else if first_imp.banner.is_some() {
            BidType::Banner
        } else {
            BidType::Native
        };

        let imp_to_cpm_override = map_imp_id_to_cpm_override(&internal_request.imp);
        let cmp_override = cmp_override_from_bid_request(internal_request);

        for sb in bid_resp.seatbid {
            let buyer: i64 = sb.buyer.parse().unwrap_or(0);
            for rb in sb.bid {
                let RubiconBid { mut bid, adm_native } = rb;

                if let Some(ext) = update_bid_ext_with_meta(&bid, buyer, &sb.seat) {
                    bid.ext = Some(ext);
                }
                let mut bid_cmp_override = imp_to_cpm_override.get(&bid.impid).copied().unwrap_or(0.0);
                if bid_cmp_override == 0.0 {
                    bid_cmp_override = cmp_override;
                }
                if bid_cmp_override > 0.0 {
                    bid.price = bid_cmp_override;
                }

                if bid.price != 0.0 {
                    // Rubicon XAPI returns one bid per response: copy response.bidid to the bid id.
                    if bid.id == "0" {
                        bid.id = bid_resp.bidid.clone();
                    }
                    let resolved_adm = resolve_adm(&bid.adm, adm_native.as_ref());
                    if !resolved_adm.is_empty() {
                        bid.adm = resolved_adm;
                    }
                    out.bids.push(TypedBid::new(bid, bid_type));
                }
            }
        }
        if !bid_resp.cur.is_empty() {
            out.currency = bid_resp.cur;
        }
        (Some(out), vec![])
    }
}

impl Adapter {
    /// One iteration of Go's `MakeRequests` loop; returns the body and impIDs.
    fn build_request(
        &self,
        request: &BidRequest,
        imp: &mut Imp,
        bidder_ext: &RubiconExtImpBidder,
        max_bids: Option<i64>,
        req_info: &ExtraRequestInfo,
    ) -> Result<(Vec<u8>, Vec<String>), BidderError> {
        let rubicon_ext = &bidder_ext.bidder;
        let mut rr = request.clone();

        let target =
            self.update_imp_rp_target(bidder_ext, rubicon_ext, imp, request.site.as_ref(), request.app.as_ref())?;
        let site_id = rubicon_ext.site_id.int64()?;
        let zone_id = rubicon_ext.zone_id.int64()?;

        let imp_ext = RubiconImpExt {
            rp: RubiconImpExtRp {
                zone_id,
                target,
                track: RubiconImpExtRpTrack { mint: String::new(), mint_version: String::new() },
            },
            gpid: bidder_ext.gpid.clone(),
            skadn: bidder_ext.skadn.clone(),
            tid: bidder_ext.tid.clone(),
            maxbids: max_bids,
        };
        imp.ext = Some(to_ext(&imp_ext)?);
        imp.secure = Some(1);

        let resolved = resolve_bid_floor(imp.bidfloor, &imp.bidfloorcur, req_info).map_err(|_| {
            BidderError::bad_input(format!(
                "Unable to convert provided bid floor currency from {} to USD",
                imp.bidfloorcur
            ))
        })?;
        if resolved >= 0.0 {
            imp.bidfloor = resolved;
            if !imp.bidfloorcur.is_empty() {
                imp.bidfloorcur = "USD".to_string();
            }
        }

        if let Some(user) = &request.user {
            let mut user_copy = user.clone();
            let target = update_user_rp_target_with_fpd_attributes(
                rubicon_ext.visitor.as_ref().map(|r| r.get().as_bytes()),
                &user_copy,
            )?;
            let mut user_ext = RubiconUserExt { eids: Vec::new(), rp: RubiconUserExtRp { target }, consent: String::new() };
            if !user_copy.eids.is_empty() {
                user_ext.eids = user_copy.eids.clone();
            }
            if !user_copy.consent.is_empty() {
                user_ext.consent = std::mem::take(&mut user_copy.consent);
            }
            user_copy.ext = Some(to_ext(&user_ext)?);
            user_copy.geo = None;
            user_copy.yob = 0;
            user_copy.gender = String::new();
            user_copy.eids = Vec::new();
            rr.user = Some(user_copy);
        }

        if let Some(device) = &request.device {
            let mut device_copy: Device = device.clone();
            let device_ext = RubiconDeviceExt { rp: RubiconDeviceExtRp { pixelratio: device.pxratio } };
            device_copy.ext = to_ext(&device_ext).ok();
            rr.device = Some(device_copy);
        }

        let video_imp = is_video(imp);
        let mut imp_type = BidType::Video;
        let mut request_native = Map::new();
        if video_imp {
            let mut video_copy = imp.video.clone().expect("video imp");
            // if imp.rwdd = 1, set imp.video.ext.videotype = "rewarded"
            let mut video_type = String::new();
            if imp.rwdd == 1 {
                video_type = "rewarded".to_string();
                imp.rwdd = 0;
            }
            let video_ext = RubiconVideoExt {
                skip: rubicon_ext.video.skip,
                skipdelay: rubicon_ext.video.skipdelay,
                videotype: video_type,
                rp: RubiconVideoExtRp { size_id: rubicon_ext.video.video_size_id },
            };
            video_copy.ext = to_ext(&video_ext).ok();
            imp.video = Some(video_copy);
            imp.banner = None;
            imp.native = None;
        } else if let Some(banner) = &imp.banner {
            let mut banner_copy: Banner = banner.clone();
            // Go: `len(Format) < 1 && (W == nil || *W == 0 && H == nil || *H == 0)`.
            // It dereferences a nil H when W is set and non-zero; report that case as invalid too.
            let invalid_size = match (banner_copy.w, banner_copy.h) {
                (None, _) => true,
                (Some(0), None) => true,
                (Some(_), None) => true,
                (Some(_), Some(h)) => h == 0,
            };
            if banner_copy.format.is_empty() && invalid_size {
                return Err(BidderError::bad_input("rubicon imps must have at least one imp.format element"));
            }
            banner_copy.ext = Some(
                Ext::from_slice(br#"{"rp":{"mime":"text/html"}}"#).map_err(|e| BidderError::other(e.to_string()))?,
            );
            imp.banner = Some(banner_copy);
            imp.video = None;
            imp.native = None;
            imp_type = BidType::Banner;
        } else {
            resolve_native_object(imp.native.as_ref(), &mut request_native)?;
            imp.video = None;
            imp_type = BidType::Native;
        }

        let account_id = rubicon_ext.account_id.int64()?;
        let pub_ext = RubiconPubExt { rp: RubiconPubExtRp { account_id } };

        if let Some(site) = &request.site {
            let mut site_copy = site.clone();
            let mut site_ext_rp = RubiconSiteExtRp { site_id, target: None };
            if let Some(content) = &site_copy.content {
                let mut site_target = Map::new();
                update_ext_with_iab_attribute(&mut site_target, &content.data, &[1, 2, 5, 6]);
                if !site_target.is_empty() {
                    site_ext_rp.target = Some(Value::Object(site_target.into_iter().collect()));
                }
            }
            site_copy.ext = Some(to_ext(&RubiconSiteExt { rp: site_ext_rp })?);
            site_copy.publisher = Some(Publisher { ext: to_ext(&pub_ext).ok(), ..Default::default() });
            rr.site = Some(site_copy);
        } else {
            // Go dereferences `request.App` here; a request with neither is rejected earlier.
            let mut app_copy = request
                .app
                .clone()
                .ok_or_else(|| BidderError::bad_input("request has neither site nor app"))?;
            app_copy.ext = Some(
                to_ext(&RubiconSiteExt { rp: RubiconSiteExtRp { site_id, target: None } })
                    .map_err(|e| BidderError::bad_input(e.to_string()))?,
            );
            app_copy.publisher = Some(Publisher {
                ext: Some(to_ext(&pub_ext).map_err(|e| BidderError::bad_input(e.to_string()))?),
                ..Default::default()
            });
            rr.app = Some(app_copy);
        }

        if let Some(source) = &request.source {
            if let Some(schain) = &source.schain {
                let mut source_copy = source.clone();
                if source_copy.ext.is_some() {
                    // Parsed for its errors only: the ext is replaced below.
                    let _: ExtSource = jsonutil::unmarshal(&ext_text(&source_copy.ext))
                        .map_err(|e| BidderError::bad_input(e.to_string()))?;
                }
                let source_ext = ExtSource { schain: Some(schain.clone()) };
                source_copy.schain = None;
                source_copy.ext = Some(to_ext(&source_ext)?);
                rr.source = Some(source_copy);
            }
        }

        if let Some(regs) = &request.regs {
            if regs.gdpr.is_some() || !regs.us_privacy.is_empty() {
                let mut regs_copy = regs.clone();
                let mut regs_ext: ExtRegs = if regs_copy.ext.is_some() {
                    jsonutil::unmarshal(&ext_text(&regs_copy.ext))
                        .map_err(|e| BidderError::bad_input(e.to_string()))?
                } else {
                    ExtRegs::default()
                };
                if regs_copy.gdpr.is_some() {
                    regs_ext.gdpr = regs_copy.gdpr;
                }
                if !regs_copy.us_privacy.is_empty() {
                    regs_ext.us_privacy = regs_copy.us_privacy.clone();
                }
                regs_copy.ext = Some(to_ext(&regs_ext)?);
                regs_copy.gdpr = None;
                regs_copy.us_privacy = String::new();
                rr.regs = Some(regs_copy);
            }
        }

        if request.badv.len() > BADV_LIMIT_SIZE {
            rr.badv = Arc::from(&request.badv[..BADV_LIMIT_SIZE]);
        }

        let imp_ids = vec![imp.id.clone()];
        rr.imp = vec![imp.clone()];
        rr.cur = Vec::new();
        rr.ext = None;

        let mut body = crate::go_json::to_vec(&rr).map_err(|e| BidderError::other(e.to_string()))?;
        if imp_type == BidType::Native && !request_native.is_empty() {
            body = set_imp_native(&body, &request_native)?;
        }
        Ok((body, imp_ids))
    }
}

// ---------------------------------------------------------------------------------------------
// MakeBids helpers
// ---------------------------------------------------------------------------------------------

fn map_imp_id_to_cpm_override(imps: &[Imp]) -> BTreeMap<String, f64> {
    let mut out = BTreeMap::new();
    for imp in imps {
        let Ok(outer) = unmarshal_raw::<RawExtImpBidder>(&ext_text(&imp.ext)) else { continue };
        let Some(bidder) = outer.bidder else { continue };
        let Ok(rubicon) = jsonutil::unmarshal::<ExtImpRubicon>(bidder.get().as_bytes()) else { continue };
        out.insert(imp.id.clone(), rubicon.debug.cpmoverride);
    }
    out
}

fn cmp_override_from_bid_request(request: &BidRequest) -> f64 {
    match jsonutil::unmarshal::<BidRequestExt>(&ext_text(&request.ext)) {
        Ok(e) => e.prebid.bidders.rubicon.debug.cpmoverride,
        Err(_) => 0.0,
    }
}

/// Go `resolveAdm`: the bid's adm, else the marshalled `adm_native` (`null` when absent).
fn resolve_adm(adm: &str, adm_native: Option<&Value>) -> String {
    if !adm.is_empty() {
        return adm.to_string();
    }
    match adm_native {
        Some(v) => crate::go_json::to_vec(v).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default(),
        None => "null".to_string(),
    }
}

/// Go `updateBidExtWithMeta`: `None` when nothing changes (or the ext cannot be parsed).
fn update_bid_ext_with_meta(bid: &Bid, buyer: i64, seat: &str) -> Option<Ext> {
    if buyer <= 0 && seat.is_empty() {
        return None;
    }
    let mut bid_ext: Option<ExtPrebid> = None;
    if bid.ext.is_some() {
        bid_ext = jsonutil::unmarshal(&ext_text(&bid.ext)).ok()?;
    }
    let network_id = buyer as i32;
    let mut ext = bid_ext.unwrap_or_default();
    match ext.prebid.as_mut() {
        Some(prebid) => match prebid.meta.as_mut() {
            Some(meta) => {
                meta.network_id = network_id;
                meta.seat = seat.to_string();
            }
            None => {
                prebid.meta = Some(ExtBidPrebidMeta { network_id, seat: seat.to_string(), ..Default::default() });
            }
        },
        None => {
            ext.prebid = Some(ExtBidPrebid {
                meta: Some(ExtBidPrebidMeta { network_id, seat: seat.to_string(), ..Default::default() }),
                ..Default::default()
            });
        }
    }
    to_ext(&ext).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_tracker() {
        assert_eq!(append_tracker_to_url("http://test.url/", "prebid"), "http://test.url/?tk_xint=prebid");
        assert_eq!(
            append_tracker_to_url("http://test.url/?hello=true", "prebid"),
            "http://test.url/?hello=true&tk_xint=prebid"
        );
    }

    #[test]
    fn set_imp_native_errors() {
        let native = Map::from([("somekey".to_string(), Value::String("someValue".into()))]);
        assert_eq!(set_imp_native(b"{}", &native).unwrap_err().to_string(), "unable to find imp in json data");
        assert_eq!(
            set_imp_native(br#"{"imp":[]}"#, &native).unwrap_err().to_string(),
            "unable to find imp[0] in json data"
        );
        assert_eq!(
            set_imp_native(br#"{"imp":[{}]}"#, &native).unwrap_err().to_string(),
            "unable to find imp[0].native in json data"
        );
    }

    #[test]
    fn resolve_native_object_cases() {
        let mk = |ver: &str, request: &str| Native { ver: ver.into(), request: request.into(), ..Default::default() };
        let cases = [
            ("1.0", r#"{"eventtrackers": "someWrongValue"}"#, None),
            ("1.1", r#"{"eventtrackers": "someWrongValue"}"#, None),
            ("1", r#"{"eventtrackers": "someWrongValue"}"#, Some("Eventtrackers are not present or not of array type")),
            ("1", r#"{"eventtrackers": [], "context": "someWrongValue"}"#, Some("Context is not of int type")),
            ("1", r#"{"eventtrackers": [], "plcmttype": 2}"#, None),
            ("1", r#"{"eventtrackers": [], "context": 1}"#, Some("Plcmttype is not present or not of int type")),
        ];
        for (ver, request, expected) in cases {
            let mut target = Map::new();
            let got = resolve_native_object(Some(&mk(ver, request)), &mut target).err().map(|e| e.to_string());
            assert_eq!(got.as_deref(), expected, "{request}");
        }
        assert_eq!(
            resolve_native_object(None, &mut Map::new()).unwrap_err().to_string(),
            "Native object is not present for request"
        );
    }
}
