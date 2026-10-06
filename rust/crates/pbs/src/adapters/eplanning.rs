//! Go `adapters/eplanning/eplanning.go`.

use std::collections::HashMap;

use regex::Regex;
use serde_json::Value;

use crate::bid_types::BidType;
use crate::bidder::{Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::ortb::adcom1::DeviceType;
use crate::ortb::openrtb2::{Bid, BidRequest, Format, Imp, SupplyChain};
use crate::ortb::Ext;

const NULL_SIZE: &str = "1x1";
const DEFAULT_PAGE_URL: &str = "FILE";
const SEC: &str = "ROS";
const DFP_CLIENT_ID: &str = "1";
const REQUEST_TARGET_INVENTORY: &str = "1";
const VAST_INSTREAM: i64 = 1;
const VAST_OUTSTREAM: i64 = 2;
const VAST_VERSION_DEFAULT: &str = "3";
const VAST_DEFAULT_SIZE: &str = "640x480";
const IMP_TYPE_BANNER: i64 = 0;

const PRIORITY_MOBILE_ASC: &[&str] = &["1x1", "300x50", "320x50", "300x250"];
const PRIORITY_DESKTOP_ASC: &[&str] = &["1x1", "970x90", "970x250", "160x600", "300x600", "728x90", "300x250"];

pub struct Adapter {
    uri: String,
    testing: bool,
}

struct ExtImpEPlanning {
    client_id: String,
    ad_unit_code: String,
    size_string: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Result<Self, BidderError> {
        Ok(Self { uri: endpoint.into(), testing: false })
    }

    /// Go test helper `setTesting` (empty `{}` body instead of a random `rnd` parameter).
    pub fn set_testing(&mut self, testing: bool) {
        self.testing = testing;
    }
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

fn clean_name(name: &str) -> String {
    let s1 = Regex::new(r"_|\.|-|/").expect("re").replace_all(name, "").into_owned();
    let s2 = Regex::new(r"\)\(|\(|\)|:").expect("re").replace_all(&s1, "_").into_owned();
    Regex::new(r"^_+|_+$").expect("re").replace_all(&s2, "").into_owned()
}

fn is_mobile_device(request: &BidRequest) -> bool {
    request.device.as_ref().is_some_and(|d| {
        d.devicetype == DeviceType::MOBILE || d.devicetype == DeviceType::PHONE || d.devicetype == DeviceType::TABLET
    })
}

fn placement(imp: &Imp) -> Option<i64> {
    imp.video.as_ref().and_then(|v| v.placement).map(|p| p.0 as i64)
}

fn get_imp_type_request(request: &BidRequest) -> i64 {
    let mut imp_type = IMP_TYPE_BANNER;
    for imp in &request.imp {
        if imp.video.is_some() {
            if placement(imp) == Some(VAST_INSTREAM) {
                imp_type = VAST_INSTREAM;
            } else if imp_type == IMP_TYPE_BANNER {
                imp_type = VAST_OUTSTREAM;
            }
        }
    }
    imp_type
}

fn first_char(v: &Value) -> char {
    v.to_string().chars().next().unwrap_or('\u{0}')
}

fn verify_imp(imp: &Imp, is_mobile: bool, imp_type: i64) -> Result<ExtImpEPlanning, BidderError> {
    // Go decodes `ExtImpBidder` first (an unmarshal failure here is "decoding extImpBidder").
    let ext_val: Result<Value, String> = match imp.ext.as_ref() {
        None => Err("unexpected end of JSON input".into()),
        Some(e) => match serde_json::from_str::<Value>(&e.to_json()) {
            Ok(v) if v.is_object() || v.is_null() => Ok(v),
            Ok(v) => Err(format!("expect {{ or n, but found {}", first_char(&v))),
            Err(e) => Err(e.to_string()),
        },
    };
    let ext_val = ext_val.map_err(|err| {
        BidderError::bad_input(format!("Ignoring imp id={}, error while decoding extImpBidder, err: {err}", imp.id))
    })?;

    if imp_type > IMP_TYPE_BANNER {
        if imp_type == VAST_INSTREAM {
            if imp.video.is_none() || placement(imp) != Some(VAST_INSTREAM) {
                return Err(BidderError::bad_input(format!(
                    "Ignoring imp id={}, auction instream and imp no instream",
                    imp.id
                )));
            }
        } else if imp.video.is_none() || placement(imp) == Some(VAST_INSTREAM) {
            return Err(BidderError::bad_input(format!(
                "Ignoring imp id={}, auction outstream and imp no outstream",
                imp.id
            )));
        }
    }

    let decode_err = |msg: String| {
        BidderError::bad_input(format!("Ignoring imp id={}, error while decoding impExt, err: {msg}", imp.id))
    };
    // Keys are matched case-insensitively (json-iterator).
    let bidder: Option<&Value> = ext_val
        .as_object()
        .and_then(|m| m.iter().find(|(k, _)| k.eq_ignore_ascii_case("bidder")))
        .map(|(_, v)| v);
    let mut client_id = String::new();
    let mut ad_unit_code = String::new();
    match bidder {
        None => return Err(decode_err("unexpected end of JSON input".into())),
        Some(Value::Null) => {}
        Some(Value::Object(m)) => {
            for (k, v) in m {
                let field = |name: &str, go: &str| -> Result<Option<String>, BidderError> {
                    if !k.eq_ignore_ascii_case(name) {
                        return Ok(None);
                    }
                    match v {
                        Value::Null => Ok(Some(String::new())),
                        Value::String(s) => Ok(Some(s.clone())),
                        other => Err(decode_err(format!(
                            "cannot unmarshal openrtb_ext.ExtImpEPlanning.{go}: expects \" or n, but found {}",
                            first_char(other)
                        ))),
                    }
                };
                if let Some(s) = field("ci", "ClientID")? {
                    client_id = s;
                }
                if let Some(s) = field("adunit_code", "AdUnitCode")? {
                    ad_unit_code = s;
                }
            }
        }
        Some(other) => return Err(decode_err(format!("expect {{ or n, but found {}", first_char(other)))),
    }
    let mut imp_ext = ExtImpEPlanning { client_id, ad_unit_code, size_string: String::new() };

    if imp_ext.client_id.is_empty() {
        return Err(BidderError::bad_input(format!("Ignoring imp id={}, no ClientID present", imp.id)));
    }

    let (width, height) = get_size_from_imp(imp, is_mobile);
    if width == 0 && height == 0 {
        imp_ext.size_string = if imp.video.is_some() { VAST_DEFAULT_SIZE.into() } else { NULL_SIZE.into() };
    } else {
        imp_ext.size_string = format!("{width}x{height}");
    }
    if imp_ext.ad_unit_code.is_empty() {
        imp_ext.ad_unit_code = imp_ext.size_string.clone();
    }
    Ok(imp_ext)
}

fn search_size_priority(hashed: &HashMap<String, usize>, format: &[Format], order: &[&str]) -> (i64, i64) {
    for p in order.iter().rev() {
        if let Some(&i) = hashed.get(*p) {
            return (format[i].w, format[i].h);
        }
    }
    (format[0].w, format[0].h)
}

fn get_size_from_imp(imp: &Imp, is_mobile: bool) -> (i64, i64) {
    if let Some(v) = &imp.video {
        let (w, h) = (v.w.unwrap_or_default(), v.h.unwrap_or_default());
        if w > 0 && h > 0 {
            return (w, h);
        }
    }
    if let Some(b) = &imp.banner {
        if let (Some(w), Some(h)) = (b.w, b.h) {
            return (w, h);
        }
        // Go would panic on `format[0]` for an empty non-nil format list: fall through to 0x0.
        if !b.format.is_empty() {
            let mut hashed = HashMap::with_capacity(b.format.len());
            for (i, f) in b.format.iter().enumerate() {
                if f.w != 0 && f.h != 0 {
                    hashed.insert(format!("{}x{}", f.w, f.h), i);
                }
            }
            let order = if is_mobile { PRIORITY_MOBILE_ASC } else { PRIORITY_DESKTOP_ASC };
            return search_size_priority(&hashed, &b.format, order);
        }
    }
    (0, 0)
}

fn get_name_video(size: &str, index_vast: usize) -> String {
    format!("video_{size}_{index_vast}")
}

fn add_header_if_non_empty(headers: &mut Header, name: &str, value: &str) {
    if !value.is_empty() {
        headers.add(name, value);
    }
}

fn make_node_string(s: &str) -> String {
    query_escape(s).replace('+', "%20")
}

fn make_supply_chain(sc: &SupplyChain) -> String {
    if sc.nodes.is_empty() {
        return String::new();
    }
    let mut out = format!("{},{}", sc.ver, sc.complete);
    for node in &sc.nodes {
        let hp = node.hp.map(|h| h.to_string()).unwrap_or_default();
        let ext = match &node.ext {
            Some(e) => make_node_string(&e.to_json()),
            None => String::new(),
        };
        out.push_str(&format!(
            "!{},{},{},{},{},{},{}",
            make_node_string(&node.asi),
            make_node_string(&node.sid),
            hp,
            make_node_string(&node.rid),
            make_node_string(&node.name),
            make_node_string(&node.domain),
            ext
        ));
    }
    out
}

/// Go `setSchain`: returns the `sch` query value, if any.
fn schain_value(ext: &Ext) -> Result<Option<String>, BidderError> {
    let v: Value = serde_json::from_str(&ext.to_json()).map_err(|e| BidderError::FailedToUnmarshal(e.to_string()))?;
    let obj = match &v {
        Value::Object(m) => m,
        Value::Null => return Ok(None),
        other => {
            return Err(BidderError::FailedToUnmarshal(format!("expect {{ or n, but found {}", first_char(other))))
        }
    };
    let sc: SupplyChain = match obj.iter().find(|(k, _)| k.eq_ignore_ascii_case("schain")).map(|(_, v)| v) {
        None | Some(Value::Null) => SupplyChain::default(),
        Some(Value::Object(_)) => serde_json::from_value(obj.iter().find(|(k, _)| k.eq_ignore_ascii_case("schain")).map(|(_, v)| v.clone()).unwrap())
            .map_err(|e| BidderError::FailedToUnmarshal(e.to_string()))?,
        Some(other) => {
            return Err(BidderError::FailedToUnmarshal(format!(
                "cannot unmarshal openrtb_ext.ExtRequestPrebidSChain.SChain: expect {{ or n, but found {}",
                first_char(other)
            )))
        }
    };
    if sc.nodes.len() > 2 {
        return Ok(None);
    }
    let s = make_supply_chain(&sc);
    Ok(if s.is_empty() { None } else { Some(s) })
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errors = Vec::with_capacity(request.imp.len());
        let mut spaces: Vec<String> = Vec::with_capacity(request.imp.len());
        let mut total_requests = 0;
        let mut client_id = String::new();
        let is_mobile = is_mobile_device(request);
        let imp_type = get_imp_type_request(request);
        let mut index_vast = 0usize;

        for imp in &request.imp {
            let ext_imp = match verify_imp(imp, is_mobile, imp_type) {
                Ok(e) => e,
                Err(e) => {
                    errors.push(e);
                    continue;
                }
            };
            if client_id.is_empty() {
                client_id = ext_imp.client_id.clone();
            }
            total_requests += 1;
            let mut name = clean_name(&ext_imp.ad_unit_code);
            if imp.video.is_some() {
                name = get_name_video(&ext_imp.size_string, index_vast);
                spaces.push(format!("{name}:{};1", ext_imp.size_string));
                index_vast += 1;
            } else {
                spaces.push(format!("{name}:{}", ext_imp.size_string));
            }
        }
        if total_requests == 0 {
            return (vec![], errors);
        }

        let mut headers = Header::new();
        headers.add("Content-Type", "application/json");
        headers.add("Accept", "application/json");
        let mut ip = String::new();
        if let Some(d) = &request.device {
            ip = d.ip.clone();
            add_header_if_non_empty(&mut headers, "User-Agent", &d.ua);
            add_header_if_non_empty(&mut headers, "X-Forwarded-For", &ip);
            add_header_if_non_empty(&mut headers, "Accept-Language", &d.language);
            if let Some(dnt) = d.dnt {
                add_header_if_non_empty(&mut headers, "DNT", &dnt.to_string());
            }
        }

        let page_url = match &request.site {
            Some(s) if !s.page.is_empty() => s.page.clone(),
            _ => DEFAULT_PAGE_URL.to_string(),
        };

        let mut page_domain = DEFAULT_PAGE_URL.to_string();
        if let Some(site) = &request.site {
            if !site.domain.is_empty() {
                page_domain = site.domain.clone();
            } else if !site.page.is_empty() {
                match parse_hostname(&site.page) {
                    Ok(h) => page_domain = h,
                    Err(e) => {
                        errors.push(e);
                        return (vec![], errors);
                    }
                }
            }
        }
        let mut request_target = page_domain;
        if let Some(app) = &request.app {
            if !app.bundle.is_empty() {
                request_target = app.bundle.clone();
            }
        }

        let mut uri_obj = match url::Url::parse(&self.uri) {
            Ok(u) => u,
            Err(e) => {
                errors.push(BidderError::other(e.to_string()));
                return (vec![], errors);
            }
        };
        let new_path = format!("{}/{}/{}/{}/{}", uri_obj.path(), client_id, DFP_CLIENT_ID, request_target, SEC);
        uri_obj.set_path(&new_path);

        let mut query: std::collections::BTreeMap<&str, String> = std::collections::BTreeMap::new();
        query.insert("ncb", "1".into());
        if request.app.is_none() {
            query.insert("ur", page_url);
        }
        query.insert("e", spaces.join("+"));
        if let Some(u) = &request.user {
            if !u.buyeruid.is_empty() {
                query.insert("uid", u.buyeruid.clone());
            }
        }
        if !ip.is_empty() {
            query.insert("ip", ip);
        }
        let body: Vec<u8>;
        if self.testing {
            body = b"{}".to_vec();
        } else {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            query.insert("rnd", ((nanos as u64) & 0x7fff_ffff_ffff_ffff).to_string());
            body = Vec::new();
        }
        if let Some(app) = &request.app {
            if !app.name.is_empty() {
                query.insert("appn", app.name.clone());
            }
            if !app.id.is_empty() {
                query.insert("appid", app.id.clone());
            }
            if let Some(d) = &request.device {
                if !d.ifa.is_empty() {
                    query.insert("ifa", d.ifa.clone());
                }
            }
            query.insert("app", REQUEST_TARGET_INVENTORY.into());
        }
        if imp_type > 0 {
            query.insert("vctx", imp_type.to_string());
            query.insert("vv", VAST_VERSION_DEFAULT.into());
        }
        if let Some(src) = &request.source {
            if let Some(ext) = &src.ext {
                match schain_value(ext) {
                    Ok(Some(s)) => {
                        query.insert("sch", s);
                    }
                    Ok(None) => {}
                    Err(e) => {
                        errors.push(e);
                        return (vec![], errors);
                    }
                }
            }
        }
        let raw_query = query
            .iter()
            .map(|(k, v)| format!("{}={}", query_escape(k), query_escape(v)))
            .collect::<Vec<_>>()
            .join("&");
        uri_obj.set_query(Some(&raw_query));

        let data = RequestData {
            method: "GET".into(),
            uri: uri_obj.to_string(),
            body,
            headers,
            imp_ids: request.imp.iter().map(|i| i.id.clone()).collect(),
        };
        (vec![data], errors)
    }

    fn make_bids(
        &self,
        internal_request: &BidRequest,
        _external_request: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if response.status_code == 204 {
            return (None, vec![]);
        }
        let msg = || format!("Unexpected status code: {}. Run with request.debug = 1 for more info", response.status_code);
        if response.status_code == 400 {
            return (None, vec![BidderError::bad_input(msg())]);
        }
        if response.status_code != 200 {
            return (None, vec![BidderError::bad_server_response(msg())]);
        }

        let parsed = match parse_hb_response(&response.body) {
            Ok(p) => p,
            Err(e) => {
                return (
                    None,
                    vec![BidderError::bad_server_response(format!("Error unmarshaling HB response: {e}"))],
                )
            }
        };

        let is_mobile = is_mobile_device(internal_request);
        let imp_type = get_imp_type_request(internal_request);
        let mut bid_response = BidderResponse::new();

        let mut space_name_to_imp_id: HashMap<String, String> = HashMap::new();
        let mut index_vast = 0usize;
        for imp in &internal_request.imp {
            let Ok(ext_imp) = verify_imp(imp, is_mobile, imp_type) else { continue };
            let mut name = clean_name(&ext_imp.ad_unit_code);
            if imp.video.is_some() {
                name = get_name_video(&ext_imp.size_string, index_vast);
                index_vast += 1;
            }
            space_name_to_imp_id.insert(name, imp.id.clone());
        }

        for (space_name, ads) in parsed {
            for ad in ads {
                if let Ok(price) = ad.price.parse::<f64>() {
                    let mut bid = Bid {
                        id: ad.impression_id,
                        adid: ad.ad_id,
                        impid: space_name_to_imp_id.get(&space_name).cloned().unwrap_or_default(),
                        price,
                        adm: ad.adm,
                        crid: ad.crid,
                        w: ad.width as i64,
                        h: ad.height as i64,
                        ..Default::default()
                    };
                    if !ad.adomain.is_empty() {
                        bid.adomain = vec![ad.adomain];
                    }
                    let t = if imp_type > 0 { BidType::Video } else { BidType::Banner };
                    bid_response.bids.push(TypedBid::new(bid, t));
                }
            }
        }
        (Some(bid_response), vec![])
    }
}

struct HbAd {
    impression_id: String,
    ad_id: String,
    price: String,
    adm: String,
    crid: String,
    adomain: String,
    width: u64,
    height: u64,
}

/// Go `hbResponse` decoding (json-iterator): returns `(space name, ads)` pairs.
fn parse_hb_response(body: &[u8]) -> Result<Vec<(String, Vec<HbAd>)>, String> {
    let v: Value = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(e) => {
            return match body.iter().find(|b| !b" \t\r\n".contains(b)) {
                Some(&c) if c != b'{' && c != b'n' => Err(format!("expect {{ or n, but found {}", c as char)),
                _ => Err(e.to_string()),
            }
        }
    };
    let obj = match &v {
        Value::Object(m) => m,
        Value::Null => return Ok(vec![]),
        other => return Err(format!("expect {{ or n, but found {}", first_char(other))),
    };
    let get = |m: &serde_json::Map<String, Value>, k: &str| -> Option<Value> {
        m.iter().find(|(key, _)| key.eq_ignore_ascii_case(k)).map(|(_, v)| v.clone())
    };
    let s = |m: &serde_json::Map<String, Value>, k: &str, go: &str| -> Result<String, String> {
        match get(m, k) {
            None | Some(Value::Null) => Ok(String::new()),
            Some(Value::String(s)) => Ok(s),
            Some(o) => Err(format!("cannot unmarshal {go}: expects \" or n, but found {}", first_char(&o))),
        }
    };
    let u = |m: &serde_json::Map<String, Value>, k: &str, go: &str| -> Result<u64, String> {
        match get(m, k) {
            None | Some(Value::Null) => Ok(0),
            Some(Value::Number(n)) => n.as_u64().ok_or_else(|| format!("cannot unmarshal {go}: unexpected character")),
            Some(o) => Err(format!("cannot unmarshal {go}: unexpected character: {}", first_char(&o))),
        }
    };
    let mut out = Vec::new();
    if let Some(Value::Array(spaces)) = get(obj, "sp") {
        for sp in spaces {
            let Value::Object(sp) = sp else { continue };
            let name = s(&sp, "k", "eplanning.hbResponseSpace.Name")?;
            let mut ads = Vec::new();
            if let Some(Value::Array(list)) = get(&sp, "a") {
                for ad in list {
                    let Value::Object(ad) = ad else { continue };
                    ads.push(HbAd {
                        impression_id: s(&ad, "i", "eplanning.hbResponseAd.ImpressionID")?,
                        ad_id: s(&ad, "id", "eplanning.hbResponseAd.AdID")?,
                        price: s(&ad, "pr", "eplanning.hbResponseAd.Price")?,
                        adm: s(&ad, "adm", "eplanning.hbResponseAd.AdM")?,
                        crid: s(&ad, "crid", "eplanning.hbResponseAd.CrID")?,
                        adomain: s(&ad, "adom", "eplanning.hbResponseAd.Adomain")?,
                        width: u(&ad, "w", "eplanning.hbResponseAd.Width")?,
                        height: u(&ad, "h", "eplanning.hbResponseAd.Height")?,
                    });
                }
            }
            out.push((name, ads));
        }
    }
    Ok(out)
}

/// Go `url.Parse(page)` then `Hostname()`, with Go's invalid-escape error text.
fn parse_hostname(page: &str) -> Result<String, BidderError> {
    let bytes = page.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let ok = i + 2 < bytes.len() + 0
                && i + 2 <= bytes.len() - 1 + 0
                && bytes[i + 1].is_ascii_hexdigit()
                && bytes[i + 2].is_ascii_hexdigit();
            if !ok {
                let end = (i + 3).min(page.len());
                return Err(BidderError::other(format!(
                    "parse \"{page}\": invalid URL escape \"{}\"",
                    &page[i..end]
                )));
            }
            i += 3;
        } else {
            i += 1;
        }
    }
    match url::Url::parse(page) {
        Ok(u) => Ok(u.host_str().unwrap_or("").trim_start_matches('[').trim_end_matches(']').to_string()),
        Err(e) => Err(BidderError::other(format!("parse \"{page}\": {e}"))),
    }
}
