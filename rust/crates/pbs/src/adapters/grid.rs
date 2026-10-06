//! Go `adapters/grid/grid.go`.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::bid_types::BidType;
use crate::bidder::{
    Bidder, BidderResponse,
    ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{Bid, BidRequest, Imp};
use crate::ortb::Ext;

use crate::bid_types::ExtBidPrebidMeta;

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


/// Go `openrtb_ext.ExtImpGrid`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
#[allow(dead_code)]
struct ExtImpGrid {
    uid: i64,
    keywords: Option<Ext>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ExtImpDataAdServer {
    name: String,
    adslot: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ExtImpData {
    pbadslot: String,
    adserver: Option<ExtImpDataAdServer>,
}

/// Go `grid.ExtImp` (what `setImpExtData` decodes and re-marshals).
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ExtImp {
    prebid: Option<Map<String, Value>>,
    bidder: Option<Value>,
    data: Option<ExtImpData>,
    gpid: String,
    skadn: Option<Value>,
    context: Option<Value>,
}

/// `openrtb_ext.ExtImpPrebid` keeps only these keys when it is marshalled again.
const PREBID_KEYS: &[&str] = &[
    "storedrequest",
    "storedauctionresponse",
    "storedbidresponse",
    "is_rewarded_inventory",
    "bidder",
    "options",
    "adunitcode",
    "passthrough",
    "floors",
    "imp",
];

// ---- keywords -------------------------------------------------------------------------------

type Segment = (String, String);
type PublisherItem = (String, Vec<Segment>);
type Publisher = BTreeMap<String, Vec<PublisherItem>>;
type Keywords = BTreeMap<String, Publisher>;

fn parse_ext_to_map(ext: Option<&Ext>) -> Map<String, Value> {
    match ext {
        Some(e) => serde_json::from_str::<Map<String, Value>>(&e.to_json()).unwrap_or_default(),
        None => Map::new(),
    }
}

fn read_embedded_map<'a>(m: &'a Map<String, Value>, k: &str) -> Option<&'a Map<String, Value>> {
    m.get(k).and_then(Value::as_object)
}

fn extract_keywords_map(ext: &Map<String, Value>) -> Map<String, Value> {
    read_embedded_map(ext, "keywords").cloned().unwrap_or_default()
}

fn extract_bidder_keywords_map(ext: &Map<String, Value>) -> Map<String, Value> {
    match read_embedded_map(ext, "bidder") {
        Some(bidder) => extract_keywords_map(bidder),
        None => Map::new(),
    }
}

fn parse_keywords_from_section(section: &Map<String, Value>) -> Publisher {
    let mut publishers = Publisher::new();
    for (publisher_key, publisher_value) in section {
        let Some(items) = publisher_value.as_array() else { continue };
        for item in items {
            let Some(item) = item.as_object() else { continue };
            let Some(publisher_name) = item.get("name").and_then(Value::as_str) else { continue };
            let mut segments: Vec<Segment> = vec![];
            if let Some(Value::Array(segs)) = item.get("segments") {
                for segment in segs {
                    if let Some(seg) = segment.as_object() {
                        if let (Some(name), Some(value)) =
                            (seg.get("name").and_then(Value::as_str), seg.get("value").and_then(Value::as_str))
                        {
                            segments.push((name.to_string(), value.to_string()));
                        }
                    }
                }
            }
            // keys are sorted (the serde_json map is ordered) for a consistent result
            for (potential_name, potential_values) in item {
                if let Some(values) = potential_values.as_array() {
                    for v in values {
                        if let Some(s) = v.as_str() {
                            segments.push((potential_name.clone(), s.to_string()));
                        }
                    }
                }
            }
            if !segments.is_empty() {
                publishers.entry(publisher_key.clone()).or_default().push((publisher_name.to_string(), segments));
            }
        }
    }
    publishers
}

fn parse_keywords_from_map(ext_keywords: &Map<String, Value>) -> Keywords {
    let mut keywords = Keywords::new();
    for (k, v) in ext_keywords {
        if k != "site" && k != "user" {
            continue;
        }
        if let Some(section) = v.as_object() {
            keywords.insert(k.clone(), parse_keywords_from_section(section));
        }
    }
    keywords
}

fn parse_keywords_from_openrtb(keywords: &str, section: &str) -> Keywords {
    let segments: Vec<Segment> =
        keywords.split(',').filter(|v| !v.is_empty()).map(|v| ("keywords".to_string(), v.to_string())).collect();
    let mut out = Keywords::new();
    if !segments.is_empty() {
        let mut publisher = Publisher::new();
        publisher.insert("ortb2".to_string(), vec![("keywords".to_string(), segments)]);
        out.insert(section.to_string(), publisher);
    }
    out
}

fn merge_keywords(a: &mut Keywords, b: Keywords) {
    for (key, values) in b {
        let section = a.entry(key).or_default();
        for (publisher_key, mut publisher_values) in values {
            let existing = section.remove(&publisher_key).unwrap_or_default();
            publisher_values.extend(existing);
            section.insert(publisher_key, publisher_values);
        }
    }
}

fn publisher_to_value(p: &Publisher) -> Value {
    let mut m = Map::new();
    for (k, items) in p {
        let arr: Vec<Value> = items
            .iter()
            .map(|(name, segs)| {
                json!({
                    "name": name,
                    "segments": segs.iter().map(|(n, v)| json!({"name": n, "value": v})).collect::<Vec<_>>(),
                })
            })
            .collect();
        m.insert(k.clone(), Value::Array(arr));
    }
    Value::Object(m)
}

/// Go `buildConsolidatedKeywordsReqExt`.
fn build_consolidated_keywords_req_ext(
    openrtb_user: &str,
    openrtb_site: &str,
    first_imp_ext: Option<&Ext>,
    request_ext: Option<&Ext>,
) -> Result<Option<Ext>, BidderError> {
    let mut request_ext_map = parse_ext_to_map(request_ext);
    let first_imp_ext_map = parse_ext_to_map(first_imp_ext);
    let mut request_ext_keywords_map = extract_keywords_map(&request_ext_map);
    let first_imp_ext_keywords_map = extract_bidder_keywords_map(&first_imp_ext_map);

    let mut keywords = parse_keywords_from_map(&request_ext_keywords_map);
    merge_keywords(&mut keywords, parse_keywords_from_map(&first_imp_ext_keywords_map));
    merge_keywords(&mut keywords, parse_keywords_from_openrtb(openrtb_user, "user"));
    merge_keywords(&mut keywords, parse_keywords_from_openrtb(openrtb_site, "site"));

    match keywords.get("site") {
        Some(site) if !site.is_empty() => {
            request_ext_keywords_map.insert("site".to_string(), publisher_to_value(site));
        }
        _ => {
            request_ext_keywords_map.remove("site");
        }
    }
    match keywords.get("user") {
        Some(user) if !user.is_empty() => {
            request_ext_keywords_map.insert("user".to_string(), publisher_to_value(user));
        }
        _ => {
            request_ext_keywords_map.remove("user");
        }
    }
    if !request_ext_keywords_map.is_empty() {
        request_ext_map.insert("keywords".to_string(), Value::Object(request_ext_keywords_map));
    } else {
        request_ext_map.remove("keywords");
    }
    if !request_ext_map.is_empty() {
        return Ok(Some(ext_from(&request_ext_map)?));
    }
    Ok(None)
}

// ---- imps -----------------------------------------------------------------------------------

fn process_imp(imp: &Imp) -> Result<(), BidderError> {
    let ext: ExtImpBidder = decode_ext(imp.ext.as_ref())?;
    let grid_ext: ExtImpGrid = decode_ext(ext.bidder.as_ref())?;
    if grid_ext.uid == 0 {
        return Err(BidderError::bad_input("uid is empty"));
    }
    Ok(())
}

fn set_imp_ext_data(mut imp: Imp) -> Imp {
    let Ok(ext) = decode_ext::<ExtImp>(imp.ext.as_ref()) else { return imp };
    let adslot = ext.data.as_ref().and_then(|d| d.adserver.as_ref()).map(|a| a.adslot.clone()).unwrap_or_default();
    if adslot.is_empty() {
        return imp;
    }
    let mut out = Map::new();
    if let Some(prebid) = &ext.prebid {
        let mut p = Map::new();
        for (k, v) in prebid {
            if PREBID_KEYS.contains(&k.as_str()) {
                p.insert(k.clone(), v.clone());
            }
        }
        out.insert("prebid".into(), Value::Object(p));
    }
    out.insert("bidder".into(), ext.bidder.clone().unwrap_or(Value::Null));
    if let Some(data) = &ext.data {
        let mut d = Map::new();
        if !data.pbadslot.is_empty() {
            d.insert("pbadslot".into(), Value::String(data.pbadslot.clone()));
        }
        if let Some(a) = &data.adserver {
            d.insert("adserver".into(), json!({"name": a.name, "adslot": a.adslot}));
        }
        out.insert("data".into(), Value::Object(d));
    }
    out.insert("gpid".into(), Value::String(adslot));
    if let Some(s) = &ext.skadn {
        out.insert("skadn".into(), s.clone());
    }
    if let Some(c) = &ext.context {
        out.insert("context".into(), c.clone());
    }
    if let Ok(e) = ext_from(&out) {
        imp.ext = Some(e);
    }
    imp
}

/// Go `fixNative`: moves `imp[].native.request` to `request_native` (parsed when it is a JSON
/// object). On any parse failure of the whole request the input is returned unchanged.
fn fix_native(req: Vec<u8>) -> Result<Vec<u8>, BidderError> {
    let Ok(mut grid_req) = serde_json::from_slice::<Map<String, Value>>(&req) else { return Ok(req) };
    // Go reuses one `parsedRequest` map across all imps, so later parses merge into earlier ones.
    let mut parsed_request: Option<Map<String, Value>> = None;
    if let Some(Value::Array(imps)) = grid_req.get_mut("imp") {
        for imp in imps {
            let Some(native) = imp.get_mut("native").and_then(Value::as_object_mut) else { continue };
            // Go always writes `"request":""`; a missing key reads as that here.
            let request = match native.get("request") {
                Some(Value::String(s)) => s.clone(),
                Some(_) => continue,
                None => String::new(),
            };
            native.remove("request");
            match jsonutil::unmarshal::<Map<String, Value>>(request.as_bytes()) {
                Ok(m) => {
                    match parsed_request.as_mut() {
                        Some(existing) => existing.extend(m),
                        None => parsed_request = Some(m),
                    }
                    native.insert(
                        "request_native".into(),
                        parsed_request.clone().map(Value::Object).unwrap_or(Value::Null),
                    );
                }
                Err(_) => {
                    native.insert("request_native".into(), Value::String(request));
                }
            }
        }
    }
    marshal(&grid_req)
}

// ---- response -------------------------------------------------------------------------------

#[derive(Debug, Default, Deserialize)]
struct GridBid {
    #[serde(flatten)]
    bid: Bid,
    #[serde(default)]
    adm_native: Option<Ext>,
    #[serde(default)]
    content_type: String,
}

#[derive(Debug, Default, Deserialize)]
struct GridSeatBid {
    #[serde(default)]
    bid: Vec<GridBid>,
}

#[derive(Debug, Default, Deserialize)]
struct GridResponse {
    #[serde(default)]
    seatbid: Vec<GridSeatBid>,
}

fn get_bid_meta(ext: Option<&Ext>) -> Option<ExtBidPrebidMeta> {
    use sonic_rs::JsonValueTrait;
    let ext = ext?;
    // Go: GridBidExt{bidder:{grid:{demandSource}}}; any decode error means no meta.
    let v: Value = serde_json::from_str(&ext.to_json()).ok()?;
    let _ = ext.0.is_object();
    let demand = v.get("bidder")?.get("grid")?.get("demandSource")?.as_str()?;
    if demand.is_empty() {
        return None;
    }
    Some(ExtBidPrebidMeta { network_name: demand.to_string(), ..Default::default() })
}

fn get_media_type_for_imp(imp_id: &str, imps: &[Imp], content_type: &str) -> Result<BidType, BidderError> {
    if !content_type.is_empty() {
        // Go passes any string through as the bid type; unknown values are rejected here.
        return BidType::parse(content_type).map_err(BidderError::other);
    }
    for imp in imps {
        if imp.id == imp_id {
            if imp.banner.is_some() {
                return Ok(BidType::Banner);
            }
            if imp.video.is_some() {
                return Ok(BidType::Video);
            }
            if imp.native.is_some() {
                return Ok(BidType::Native);
            }
            return Err(BidderError::bad_server_response(format!("Unknown impression type for ID: \"{imp_id}\"")));
        }
    }
    // This shouldnt happen. Lets handle it just incase by returning an error.
    Err(BidderError::bad_server_response(format!("Failed to find impression for ID: \"{imp_id}\"")))
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errors: Vec<BidderError> = vec![];
        let mut valid_imps = vec![];
        for imp in &request.imp {
            match process_imp(imp) {
                Ok(()) => valid_imps.push(set_imp_ext_data(imp.clone())),
                Err(e) => errors.push(e),
            }
        }
        if valid_imps.is_empty() {
            errors.push(BidderError::bad_input("No valid impressions for grid"));
            return (vec![], errors);
        }

        let mut req = request.clone();
        // setImpExtKeywords reads imp[0] before the imps are replaced.
        let user_keywords = req.user.as_ref().map(|u| u.keywords.as_str()).unwrap_or("");
        let site_keywords = req.site.as_ref().map(|s| s.keywords.as_str()).unwrap_or("");
        match build_consolidated_keywords_req_ext(
            user_keywords,
            site_keywords,
            request.imp.first().and_then(|i| i.ext.as_ref()),
            request.ext.as_ref(),
        ) {
            Ok(e) => req.ext = e,
            Err(e) => {
                errors.push(e);
                return (vec![], errors);
            }
        }
        req.imp = valid_imps;

        let req_json = match marshal(&req) {
            Ok(b) => b,
            Err(e) => {
                errors.push(e);
                return (vec![], errors);
            }
        };
        let fixed = match fix_native(req_json) {
            Ok(b) => b,
            Err(e) => {
                errors.push(e);
                return (vec![], errors);
            }
        };
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        (
            vec![RequestData {
                method: "POST".into(),
                uri: self.endpoint.clone(),
                body: fixed,
                headers,
                imp_ids: imp_ids(&req.imp),
            }],
            errors,
        )
    }

    fn make_bids(
        &self,
        internal_request: &BidRequest,
        _request_data: &RequestData,
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
        let bid_resp: GridResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut out = BidderResponse::with_bids_capacity(1);
        for sb in bid_resp.seatbid {
            for gb in sb.bid {
                let mut bid = gb.bid;
                let bid_meta = get_bid_meta(bid.ext.as_ref());
                let bid_type = match get_media_type_for_imp(&bid.impid, &internal_request.imp, &gb.content_type) {
                    Ok(t) => t,
                    Err(e) => return (None, vec![e]),
                };
                if let Some(native) = &gb.adm_native {
                    if bid.adm.is_empty() {
                        bid.adm = native.to_json();
                    }
                }
                let mut tb = TypedBid::new(bid, bid_type);
                tb.bid_meta = bid_meta;
                out.bids.push(tb);
            }
        }
        (Some(out), vec![])
    }
}
