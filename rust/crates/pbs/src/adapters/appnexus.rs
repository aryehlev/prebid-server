//! Go `adapters/appnexus/appnexus.go`, `models.go`, `iab_categories.go` and
//! `openrtb_ext/imp_appnexus.go`.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

use crate::bid_types::{BidType, ExtBidPrebidVideo};
use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder,
    BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::config;
use crate::errortypes::BidderError;
use crate::ext_helpers::ext_str;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::adcom1::PlacementPosition;
use crate::ortb::openrtb2::{BidRequest, BidResponse, Imp};
use crate::ortb::Ext;

const DEFAULT_PLATFORM_ID: i32 = 5;
const MAX_IMPS_PER_REQ: usize = 10;

/// Go `randomutil.RandomGenerator.GenerateInt63`.
pub type RandomGenerator = Box<dyn Fn() -> i64 + Send + Sync>;

pub struct Adapter {
    uri: String,
    hb_source: i32,
    random_generator: RandomGenerator,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(cfg: &config::Adapter) -> Result<Self, String> {
        // Go `url.Parse` accepts relative references, the `url` crate does not.
        match url::Url::parse(&cfg.endpoint) {
            Ok(_) | Err(url::ParseError::RelativeUrlWithoutBase) => {}
            Err(e) => return Err(e.to_string()),
        }
        Ok(Self {
            uri: cfg.endpoint.clone(),
            hb_source: resolve_platform_id(&cfg.platform_id),
            random_generator: Box::new(default_random_int63),
        })
    }

    /// Replaces the ad pod id generator (Go tests swap `randomGenerator` for a fake).
    pub fn with_random_generator(mut self, generator: RandomGenerator) -> Self {
        self.random_generator = generator;
        self
    }
}

fn default_random_int63() -> i64 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u8(0);
    (h.finish() >> 1) as i64
}

/// Go `resolvePlatformID` (`strconv.Atoi`).
fn resolve_platform_id(platform_id: &str) -> i32 {
    if !platform_id.is_empty() {
        if let Ok(v) = platform_id.parse::<i64>() {
            return v as i32;
        }
    }
    DEFAULT_PLATFORM_ID
}

// ---- openrtb_ext.ExtImpAppnexus ------------------------------------------------------------

/// Go `jsonutil.StringInt`: a number or a numeric string.
fn de_string_int<'de, D: Deserializer<'de>>(d: D) -> Result<i64, D::Error> {
    use serde::de::Error;
    match Value::deserialize(d)? {
        Value::Number(n) => n.as_i64().ok_or_else(|| D::Error::custom(format!("Value is not a number: {n}"))),
        Value::String(s) => {
            if s.is_empty() {
                return Ok(0);
            }
            parse_int(&s).ok_or_else(|| D::Error::custom(format!("Value is not a number: {s}")))
        }
        other => Err(D::Error::custom(format!("Value is not a number: {other}"))),
    }
}

/// Go `jsonparser.ParseInt`: optional sign then digits.
fn parse_int(s: &str) -> Option<i64> {
    let digits = s.strip_prefix(['-', '+']).unwrap_or(s);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse::<i64>().ok()
}

/// Go `jsonutil.IntString`: a number or a string, kept as its text.
fn de_int_string<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    use serde::de::Error;
    match Value::deserialize(d)? {
        Value::Number(n) => Ok(n.to_string()),
        Value::String(s) => Ok(s),
        _ => Err(D::Error::custom("invalid type")),
    }
}

/// Go `ExtImpAppnexusKeywords.UnmarshalJSON`.
fn de_keywords<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    use serde::de::Error;
    fn join(parts: Vec<String>) -> String {
        let mut s = parts.concat();
        s.pop();
        s
    }
    match Value::deserialize(d)? {
        Value::Object(map) => {
            let results: BTreeMap<String, Vec<String>> =
                serde_json::from_value(Value::Object(map)).map_err(D::Error::custom)?;
            let mut parts = vec![];
            for (key, values) in results {
                if values.is_empty() {
                    parts.push(format!("{key},"));
                } else {
                    for v in values {
                        parts.push(format!("{key}={v},"));
                    }
                }
            }
            Ok(join(parts))
        }
        v @ Value::Array(_) => {
            #[derive(Deserialize, Default)]
            #[serde(default)]
            struct KeyVal {
                key: String,
                #[serde(rename = "value")]
                values: Vec<String>,
            }
            let results: Vec<KeyVal> = serde_json::from_value(v).map_err(D::Error::custom)?;
            let mut parts = vec![];
            for kv in results {
                if kv.values.is_empty() {
                    parts.push(format!("{},", kv.key));
                } else {
                    for v in kv.values {
                        parts.push(format!("{}={},", kv.key, v));
                    }
                }
            }
            Ok(join(parts))
        }
        Value::String(s) => Ok(s),
        // Other JSON types leave the keywords empty without an error.
        _ => Ok(String::new()),
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpAppnexus {
    #[serde(rename = "placementId", deserialize_with = "de_string_int")]
    deprecated_placement_id: i64,
    #[serde(rename = "invCode")]
    legacy_inv_code: String,
    #[serde(rename = "trafficSourceCode")]
    legacy_traffic_source_code: String,
    #[serde(rename = "placement_id", deserialize_with = "de_string_int")]
    placement_id: i64,
    inv_code: String,
    #[serde(deserialize_with = "de_int_string")]
    member: String,
    #[serde(deserialize_with = "de_keywords")]
    keywords: String,
    traffic_source_code: String,
    reserve: f64,
    position: String,
    use_pmt_rule: Option<bool>,
    use_payment_rule: Option<bool>,
    private_sizes: Option<Value>,
    generate_ad_pod_id: bool,
    ext_inv_code: String,
    external_imp_id: String,
}

/// Go `impExtIncoming`.
#[derive(Deserialize, Default)]
#[serde(default)]
struct ImpExtIncoming {
    bidder: ExtImpAppnexus,
    gpid: String,
}

// ---- models.go -----------------------------------------------------------------------------

#[derive(Serialize)]
struct ImpExt<'a> {
    appnexus: ImpExtAppnexus<'a>,
    #[serde(skip_serializing_if = "str::is_empty")]
    gpid: &'a str,
}

#[derive(Serialize)]
struct ImpExtAppnexus<'a> {
    #[serde(skip_serializing_if = "is_zero_i64")]
    placement_id: i64,
    #[serde(skip_serializing_if = "str::is_empty")]
    keywords: &'a str,
    #[serde(skip_serializing_if = "str::is_empty")]
    traffic_source_code: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    use_pmt_rule: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    private_sizes: Option<&'a Value>,
    #[serde(skip_serializing_if = "str::is_empty")]
    ext_inv_code: &'a str,
    #[serde(skip_serializing_if = "str::is_empty")]
    external_imp_id: &'a str,
}

fn is_zero_i64(v: &i64) -> bool {
    *v == 0
}
fn is_zero_i32(v: &i32) -> bool {
    *v == 0
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct BidExtVideo {
    duration: i32,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct BidExtCreative {
    video: BidExtVideo,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct BidExtAppnexus {
    bid_ad_type: i64,
    brand_id: i64,
    #[serde(rename = "brand_category_id")]
    brand_category: i64,
    creative_info: BidExtCreative,
    deal_priority: i32,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct BidExt {
    appnexus: BidExtAppnexus,
}

#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(default)]
struct BidReqExtAppnexus {
    #[serde(skip_serializing_if = "Option::is_none")]
    include_brand_category: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    brand_category_uniqueness: Option<bool>,
    #[serde(skip_serializing_if = "is_zero_i32")]
    is_amp: i32,
    #[serde(skip_serializing_if = "is_zero_i32")]
    hb_source: i32,
    #[serde(skip_serializing_if = "String::is_empty")]
    adpod_id: String,
}

// ---- adapter -------------------------------------------------------------------------------

fn ext_bytes(ext: &Option<Ext>) -> Vec<u8> {
    ext.as_ref().map(|e| e.to_json().into_bytes()).unwrap_or_default()
}

/// `jsonutil::unmarshal` with json-iterator's wording for empty input (nil ext).
fn unmarshal_obj<T: serde::de::DeserializeOwned>(data: &[u8]) -> Result<T, BidderError> {
    if data.iter().all(|b| b" \t\r\n".contains(b)) {
        return Err(BidderError::FailedToUnmarshal("expect { or n, but found \u{0}".to_string()));
    }
    jsonutil::unmarshal(data)
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request_in: &BidRequest,
        req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut request = request_in.clone();
        // The appnexus adapter expects imp.displaymanagerver to be populated in the openrtb2
        // endpoint, but some SDKs put it in imp.ext.prebid instead.
        let display_manager_ver = build_display_manager_ver(&request);

        let mut should_generate_ad_pod_id: Option<bool> = None;
        let mut unique_member_id = String::new();
        let mut errs: Vec<BidderError> = vec![];
        let mut valid_imps: Vec<Imp> = vec![];

        for i in 0..request.imp.len() {
            let imp_ext_incoming = match validate_and_build_imp_ext(&request.imp[i]) {
                Ok(e) => e,
                Err(e) => {
                    errs.push(e);
                    continue;
                }
            };
            if let Err(e) = build_request_imp(&mut request.imp[i], &imp_ext_incoming, &display_manager_ver) {
                errs.push(e);
                continue;
            }
            let member_id = imp_ext_incoming.bidder.member.clone();
            if !member_id.is_empty() {
                // The Appnexus API requires a Member ID in the URL, so the request may fail if
                // different impressions have different member IDs.
                if unique_member_id.is_empty() {
                    unique_member_id = member_id;
                } else if unique_member_id != member_id {
                    errs.push(BidderError::other(format!(
                        "all request.imp[i].ext.prebid.bidder.appnexus.member params must match. Request contained member IDs {unique_member_id} and {member_id}"
                    )));
                    return (vec![], errs);
                }
            }
            let for_imp = imp_ext_incoming.bidder.generate_ad_pod_id;
            match should_generate_ad_pod_id {
                None => should_generate_ad_pod_id = Some(for_imp),
                Some(v) if v != for_imp => {
                    errs.push(BidderError::other("generate ad pod option should be same for all pods in request"));
                    return (vec![], errs);
                }
                _ => {}
            }
            valid_imps.push(request.imp[i].clone());
        }
        request.imp = valid_imps;

        // If all the requests were malformed, don't bother making a server call with no imps.
        if request.imp.is_empty() {
            return (vec![], errs);
        }

        let request_uri = if unique_member_id.is_empty() {
            self.uri.clone()
        } else {
            append_member_id(&self.uri, &unique_member_id)
        };

        // Add the Appnexus request level extension.
        let (mut is_amp, mut is_video) = (0, 0);
        if req_info.pbs_entry_point == "amp" {
            is_amp = 1;
        } else if req_info.pbs_entry_point == "video" {
            is_video = 1;
        }

        let mut req_ext = match get_request_ext(&request.ext) {
            Ok(m) => m,
            Err(e) => {
                errs.push(e);
                return (vec![], errs);
            }
        };
        let req_ext_appnexus = match self.get_appnexus_ext(&req_ext, is_amp, is_video) {
            Ok(e) => e,
            Err(e) => {
                errs.push(e);
                return (vec![], errs);
            }
        };
        if let Err(e) = move_supply_chain(&mut request, &mut req_ext) {
            errs.push(e);
            return (vec![], errs);
        }

        // For long form requests with the adpod id feature enabled, adpod_id is sent
        // downstream; all impressions of a pod share it, and a pod over `MAX_IMPS_PER_REQ`
        // is split across requests that keep the same pod id.
        let imps = request.imp.clone();
        let (requests, errors) = if is_video == 1 && should_generate_ad_pod_id == Some(true) {
            self.build_ad_pod_requests(&imps, &mut request, &req_ext, req_ext_appnexus, &request_uri)
        } else {
            split_requests(&imps, &mut request, &req_ext, &req_ext_appnexus, &request_uri)
        };
        errs.extend(errors);
        (requests, errs)
    }

    fn make_bids(
        &self,
        _internal_request: &BidRequest,
        _external_request: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if is_response_status_code_no_content(response) {
            return (None, vec![]);
        }
        if let Some(err) = check_response_status_code_for_errors(response) {
            return (None, vec![err]);
        }
        let appnexus_response: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };

        let mut errs = vec![];
        let mut bidder_response = BidderResponse::with_bids_capacity(5);
        for sb in appnexus_response.seatbid {
            for mut bid in sb.bid {
                let bid_ext: BidExt = match unmarshal_obj(&ext_bytes(&bid.ext)) {
                    Ok(e) => e,
                    Err(e) => {
                        errs.push(e);
                        continue;
                    }
                };
                let bid_type = match get_media_type_for_bid(&bid_ext) {
                    Ok(t) => t,
                    Err(e) => {
                        errs.push(e);
                        continue;
                    }
                };
                match find_iab_category_for_bid(&bid_ext) {
                    Some(cat) => bid.cat = vec![cat.to_string()],
                    // An empty categories array forces the bid to be rejected.
                    None if bid.cat.len() > 1 => bid.cat = vec![],
                    None => {}
                }
                let mut typed = TypedBid::new(bid, bid_type);
                typed.bid_video = Some(ExtBidPrebidVideo {
                    duration: bid_ext.appnexus.creative_info.video.duration,
                    primary_category: String::new(),
                });
                typed.deal_priority = bid_ext.appnexus.deal_priority;
                bidder_response.bids.push(typed);
            }
        }
        if !appnexus_response.cur.is_empty() {
            bidder_response.currency = appnexus_response.cur;
        }
        (Some(bidder_response), errs)
    }
}

/// Go `getRequestExt`.
fn get_request_ext(ext: &Option<Ext>) -> Result<Map<String, Value>, BidderError> {
    let bytes = ext_bytes(ext);
    if !bytes.is_empty() {
        // json-iterator maps `null` to a nil map, which the adapter then writes to.
        return Ok(jsonutil::unmarshal::<Option<Map<String, Value>>>(&bytes)?.unwrap_or_default());
    }
    Ok(Map::new())
}

impl Adapter {
    /// Go `getAppnexusExt`.
    fn get_appnexus_ext(
        &self,
        ext_map: &Map<String, Value>,
        is_amp: i32,
        is_video: i32,
    ) -> Result<BidReqExtAppnexus, BidderError> {
        let mut appnexus_ext = BidReqExtAppnexus::default();
        if let Some(json) = ext_map.get("appnexus") {
            appnexus_ext = jsonutil::unmarshal(json.to_string().as_bytes())?;
        }
        if let Some(prebid) = ext_map.get("prebid") {
            if prebid.get("targeting").and_then(|t| t.get("includebrandcategory")).is_some_and(Value::is_object) {
                appnexus_ext.brand_category_uniqueness = Some(true);
                appnexus_ext.include_brand_category = Some(true);
            }
        }
        appnexus_ext.is_amp = is_amp;
        appnexus_ext.hb_source = self.hb_source + is_video;
        Ok(appnexus_ext)
    }

    fn build_ad_pod_requests(
        &self,
        imps: &[Imp],
        request: &mut BidRequest,
        request_ext: &Map<String, Value>,
        mut request_ext_appnexus: BidReqExtAppnexus,
        uri: &str,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = vec![];
        let pod_imps = group_by_pods(imps);
        let mut requests = Vec::with_capacity(pod_imps.len());
        for imps in pod_imps {
            request_ext_appnexus.adpod_id = (self.random_generator)().to_string();
            let (reqs, errors) = split_requests(&imps, request, request_ext, &request_ext_appnexus, uri);
            requests.extend(reqs);
            errs.extend(errors);
        }
        (requests, errs)
    }
}

fn validate_and_build_imp_ext(imp: &Imp) -> Result<ImpExtIncoming, BidderError> {
    let mut ext: ImpExtIncoming = unmarshal_obj(&ext_bytes(&imp.ext))?;
    handle_legacy_params(&mut ext.bidder);
    validate_appnexus_ext(&ext.bidder)?;
    Ok(ext)
}

fn handle_legacy_params(e: &mut ExtImpAppnexus) {
    if e.placement_id == 0 && e.deprecated_placement_id != 0 {
        e.placement_id = e.deprecated_placement_id;
    }
    if e.inv_code.is_empty() && !e.legacy_inv_code.is_empty() {
        e.inv_code = e.legacy_inv_code.clone();
    }
    if e.traffic_source_code.is_empty() && !e.legacy_traffic_source_code.is_empty() {
        e.traffic_source_code = e.legacy_traffic_source_code.clone();
    }
    if e.use_pmt_rule.is_none() && e.use_payment_rule.is_some() {
        e.use_pmt_rule = e.use_payment_rule;
    }
}

fn validate_appnexus_ext(e: &ExtImpAppnexus) -> Result<(), BidderError> {
    if e.placement_id == 0 && (e.inv_code.is_empty() || e.member.is_empty()) {
        return Err(BidderError::bad_input("No placement or member+invcode provided"));
    }
    Ok(())
}

/// Groups imps by the part of the imp id before the first `_`. Go iterates a map in random
/// order; pods are returned in order of first appearance.
fn group_by_pods(imps: &[Imp]) -> Vec<Vec<Imp>> {
    let mut order: Vec<String> = vec![];
    let mut pods: BTreeMap<String, Vec<Imp>> = BTreeMap::new();
    for imp in imps {
        let pod = imp.id.split('_').next().unwrap_or_default().to_string();
        if !pods.contains_key(&pod) {
            order.push(pod.clone());
        }
        pods.entry(pod).or_default().push(imp.clone());
    }
    order.into_iter().filter_map(|p| pods.remove(&p)).collect()
}

fn split_requests(
    imps: &[Imp],
    request: &mut BidRequest,
    request_ext: &Map<String, Value>,
    request_ext_appnexus: &BidReqExtAppnexus,
    uri: &str,
) -> (Vec<RequestData>, Vec<BidderError>) {
    let mut errs = vec![];
    // Initial capacity: e.g. 35 imps at 10 per request need (35 + 10 - 1) / 10 = 4 requests.
    let initial_capacity = (imps.len() + MAX_IMPS_PER_REQ - 1) / MAX_IMPS_PER_REQ;
    let mut res_arr = Vec::with_capacity(initial_capacity);
    let mut start_ind = 0;
    let mut imps_left = !imps.is_empty();

    let mut headers = Header::new();
    headers.add("Content-Type", "application/json;charset=utf-8");
    headers.add("Accept", "application/json");

    let mut request_ext_clone = request_ext.clone();
    match serde_json::to_value(request_ext_appnexus) {
        Ok(v) => {
            request_ext_clone.insert("appnexus".to_string(), v);
        }
        Err(e) => errs.push(BidderError::other(e.to_string())),
    }
    match Ext::from_serialize(&request_ext_clone) {
        Ok(e) => request.ext = Some(e),
        Err(e) => errs.push(BidderError::other(e.to_string())),
    }

    while imps_left {
        let mut end_ind = start_ind + MAX_IMPS_PER_REQ;
        if end_ind >= imps.len() {
            end_ind = imps.len();
            imps_left = false;
        }
        request.imp = imps[start_ind..end_ind].to_vec();
        let req_json = match crate::go_json::to_vec(&*request) {
            Ok(b) => b,
            Err(e) => {
                errs.push(BidderError::other(e.to_string()));
                return (vec![], errs);
            }
        };
        res_arr.push(RequestData {
            method: "POST".into(),
            uri: uri.to_string(),
            body: req_json,
            headers: headers.clone(),
            imp_ids: request.imp.iter().map(|i| i.id.clone()).collect(),
        });
        start_ind = end_ind;
    }
    (res_arr, errs)
}

fn build_request_imp(
    imp: &mut Imp,
    ext: &ImpExtIncoming,
    display_manager_ver: &str,
) -> Result<(), BidderError> {
    if !ext.bidder.inv_code.is_empty() {
        imp.tagid = ext.bidder.inv_code.clone();
    }
    if imp.bidfloor <= 0.0 && ext.bidder.reserve > 0.0 {
        imp.bidfloor = ext.bidder.reserve; // This will be broken for non-USD currency.
    }
    if let Some(banner) = imp.banner.as_mut() {
        if ext.bidder.position == "above" {
            banner.pos = Some(PlacementPosition::ABOVE_FOLD);
        } else if ext.bidder.position == "below" {
            banner.pos = Some(PlacementPosition::BELOW_FOLD);
        }
        if banner.w.is_none() && banner.h.is_none() && !banner.format.is_empty() {
            let first = &banner.format[0];
            banner.w = Some(first.w);
            banner.h = Some(first.h);
        }
    }
    // Populate imp.displaymanagerver if the SDK failed to do it.
    if imp.displaymanagerver.is_empty() && !display_manager_ver.is_empty() {
        imp.displaymanagerver = display_manager_ver.to_string();
    }
    let b = &ext.bidder;
    let imp_ext = ImpExt {
        appnexus: ImpExtAppnexus {
            placement_id: b.placement_id,
            keywords: &b.keywords,
            traffic_source_code: &b.traffic_source_code,
            use_pmt_rule: b.use_pmt_rule,
            private_sizes: b.private_sizes.as_ref(),
            ext_inv_code: &b.ext_inv_code,
            external_imp_id: &b.external_imp_id,
        },
        gpid: &ext.gpid,
    };
    imp.ext = Some(Ext::from_serialize(&imp_ext).map_err(|e| BidderError::other(e.to_string()))?);
    Ok(())
}

/// Go `getMediaTypeForBid`.
fn get_media_type_for_bid(bid: &BidExt) -> Result<BidType, BidderError> {
    match bid.appnexus.bid_ad_type {
        0 => Ok(BidType::Banner),
        1 => Ok(BidType::Video),
        3 => Ok(BidType::Native),
        other => Err(BidderError::other(format!(
            "Unrecognized bid_ad_type in response from appnexus: {other}"
        ))),
    }
}

/// Go `findIabCategoryForBid`: maps an appnexus brand id to an IAB category.
fn find_iab_category_for_bid(bid: &BidExt) -> Option<&'static str> {
    let id = bid.appnexus.brand_category.to_string();
    IAB_CATEGORIES.iter().find(|(k, _)| *k == id).map(|(_, v)| *v)
}

/// Go `appendMemberId`: sets `member_id` and re-encodes the query sorted by key
/// (`url.Values.Encode`).
fn append_member_id(uri: &str, member_id: &str) -> String {
    let (base, query) = match uri.split_once('?') {
        Some((b, q)) => (b, q),
        None => (uri, ""),
    };
    let mut values: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (k, v) in url::form_urlencoded::parse(query.as_bytes()) {
        values.entry(k.into_owned()).or_default().push(v.into_owned());
    }
    values.insert("member_id".to_string(), vec![member_id.to_string()]);
    let mut out = String::new();
    for (k, vs) in &values {
        for v in vs {
            if !out.is_empty() {
                out.push('&');
            }
            out.push_str(&query_escape(k));
            out.push('=');
            out.push_str(&query_escape(v));
        }
    }
    format!("{base}?{out}")
}

/// Go `url.QueryEscape`.
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

/// Go `buildDisplayManageVer`.
fn build_display_manager_ver(req: &BidRequest) -> String {
    let Some(app) = &req.app else { return String::new() };
    let Some(source) = ext_str(app.ext.as_ref(), &["prebid", "source"]) else { return String::new() };
    let Some(version) = ext_str(app.ext.as_ref(), &["prebid", "version"]) else { return String::new() };
    format!("{source}-{version}")
}

/// Go `moveSupplyChain`: moves `source.ext.schain` to `ext.schain`.
fn move_supply_chain(request: &mut BidRequest, ext_map: &mut Map<String, Value>) -> Result<(), BidderError> {
    let Some(source) = request.source.as_mut() else { return Ok(()) };
    let Some(source_ext) = source.ext.as_ref() else { return Ok(()) };
    let mut source_ext_map: Map<String, Value> = jsonutil::unmarshal(source_ext.to_json().as_bytes())?;
    let Some(schain) = source_ext_map.remove("schain") else { return Ok(()) };
    source.ext = if source_ext_map.is_empty() {
        None
    } else {
        Some(Ext::from_serialize(&source_ext_map).map_err(|e| BidderError::other(e.to_string()))?)
    };
    ext_map.insert("schain".to_string(), schain);
    Ok(())
}

const IAB_CATEGORIES: &[(&str, &str)] = &[
    ("1", "IAB20-3"),
    ("2", "IAB18-5"),
    ("3", "IAB10-1"),
    ("4", "IAB2-3"),
    ("5", "IAB19-8"),
    ("6", "IAB22-1"),
    ("7", "IAB18-1"),
    ("8", "IAB12-3"),
    ("9", "IAB5-1"),
    ("10", "IAB4-5"),
    ("11", "IAB13-4"),
    ("12", "IAB8-7"),
    ("13", "IAB9-7"),
    ("14", "IAB7-1"),
    ("15", "IAB20-18"),
    ("16", "IAB10-7"),
    ("17", "IAB19-18"),
    ("18", "IAB13-6"),
    ("19", "IAB18-4"),
    ("20", "IAB1-5"),
    ("21", "IAB1-6"),
    ("22", "IAB3-4"),
    ("23", "IAB19-13"),
    ("24", "IAB22-2"),
    ("25", "IAB3-9"),
    ("26", "IAB17-18"),
    ("27", "IAB19-6"),
    ("28", "IAB1-7"),
    ("29", "IAB9-30"),
    ("30", "IAB20-7"),
    ("31", "IAB20-17"),
    ("32", "IAB7-32"),
    ("33", "IAB16-5"),
    ("34", "IAB19-34"),
    ("35", "IAB11-5"),
    ("36", "IAB12-3"),
    ("37", "IAB11-4"),
    ("38", "IAB12-3"),
    ("39", "IAB9-30"),
    ("41", "IAB7-44"),
    ("42", "IAB7-1"),
    ("43", "IAB7-30"),
    ("50", "IAB19-30"),
    ("51", "IAB17-12"),
    ("52", "IAB19-30"),
    ("53", "IAB3-1"),
    ("55", "IAB13-2"),
    ("56", "IAB19-30"),
    ("57", "IAB19-30"),
    ("58", "IAB7-39"),
    ("59", "IAB22-1"),
    ("60", "IAB7-39"),
    ("61", "IAB21-3"),
    ("62", "IAB5-1"),
    ("63", "IAB12-3"),
    ("64", "IAB20-18"),
    ("65", "IAB11-2"),
    ("66", "IAB17-18"),
    ("67", "IAB9-9"),
    ("68", "IAB9-5"),
    ("69", "IAB7-44"),
    ("71", "IAB22-3"),
    ("73", "IAB19-30"),
    ("74", "IAB8-5"),
    ("78", "IAB22-1"),
    ("85", "IAB12-2"),
    ("86", "IAB22-3"),
    ("87", "IAB11-3"),
    ("112", "IAB7-32"),
    ("113", "IAB7-32"),
    ("114", "IAB7-32"),
    ("115", "IAB7-32"),
    ("118", "IAB9-5"),
    ("119", "IAB9-5"),
    ("120", "IAB9-5"),
    ("121", "IAB9-5"),
    ("122", "IAB9-5"),
    ("123", "IAB9-5"),
    ("124", "IAB9-5"),
    ("125", "IAB9-5"),
    ("126", "IAB9-5"),
    ("127", "IAB22-1"),
    ("132", "IAB1-2"),
    ("133", "IAB19-30"),
    ("137", "IAB3-9"),
    ("138", "IAB19-3"),
    ("140", "IAB2-3"),
    ("141", "IAB2-1"),
    ("142", "IAB2-3"),
    ("143", "IAB17-13"),
    ("166", "IAB11-4"),
    ("175", "IAB3-1"),
    ("176", "IAB13-4"),
    ("182", "IAB8-9"),
    ("183", "IAB3-5"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_member_id_sorts_query() {
        assert_eq!(
            append_member_id("http://ib.adnxs.com/openrtb2?query_param=true", "102"),
            "http://ib.adnxs.com/openrtb2?member_id=102&query_param=true"
        );
    }

    #[test]
    fn builder_with_platform_id() {
        let cfg = config::Adapter {
            endpoint: "http://ib.adnxs.com/openrtb2".into(),
            platform_id: "3".into(),
            ..Default::default()
        };
        assert_eq!(Adapter::new(&cfg).unwrap().hb_source, 3);
    }

    #[test]
    fn platform_id_defaults() {
        assert_eq!(resolve_platform_id(""), 5);
        assert_eq!(resolve_platform_id("abc"), 5);
    }
}
