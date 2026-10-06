//! Go `adapters/adnuntius`.
#![allow(dead_code, unused_imports)]

use crate::bid_types::BidType;
use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder, BidderResponse,
    ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::macros::{EndpointTemplate, EndpointTemplateParams};
use crate::ortb::openrtb2::{Bid, BidRequest, BidResponse, Imp};
use crate::ortb::Ext;
use sonic_rs::{JsonContainerTrait, JsonValueTrait};
use crate::ortb::openrtb2::MarkupType;
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use std::collections::BTreeMap;

const DEFAULT_NETWORK: &str = "default";
const DEFAULT_SITE: &str = "unknown";
const MINUTES_IN_HOUR: i64 = 60;

pub struct Adapter {
    endpoint: String,
    extra_info: String,
    /// Go `time.Now().Zone()` offset in seconds east of UTC (`RealTime` in production).
    tz_offset_seconds: i64,
}

impl Adapter {
    /// Go `Builder` (`config.Endpoint`, `config.ExtraAdapterInfo`). The zone offset is UTC (0)
    /// unless set with [`with_tz_offset_seconds`](Self::with_tz_offset_seconds).
    pub fn new(endpoint: impl Into<String>, extra_adapter_info: impl Into<String>) -> Self {
        Self { endpoint: endpoint.into(), extra_info: extra_adapter_info.into(), tz_offset_seconds: 0 }
    }

    /// The `timeutil.Time` the Go adapter holds: the local zone offset used for `tzo`.
    pub fn with_tz_offset_seconds(mut self, offset: i64) -> Self {
        self.tz_offset_seconds = offset;
        self
    }
}

// ---- request types ----------------------------------------------------------------------------

#[derive(Serialize, Default, Clone)]
struct NativeRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    ortb: Option<Box<RawValue>>,
}

#[derive(Serialize, Clone)]
struct AdnRequestAdunit {
    #[serde(rename = "auId")]
    au_id: String,
    #[serde(rename = "targetId")]
    target_id: String,
    #[serde(rename = "adType", skip_serializing_if = "String::is_empty")]
    ad_type: String,
    #[serde(rename = "nativeRequest")]
    native_request: NativeRequest,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    dimensions: Vec<Vec<i64>>,
    #[serde(rename = "maxDeals", skip_serializing_if = "is_zero_i64")]
    max_deals: i64,
    #[serde(rename = "c", skip_serializing_if = "Option::is_none")]
    category: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    segments: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    keywords: Option<Vec<String>>,
    #[serde(rename = "kv", skip_serializing_if = "Option::is_none")]
    key_values: Option<BTreeMap<String, Vec<String>>>,
    #[serde(rename = "auml", skip_serializing_if = "Option::is_none")]
    ad_unit_matching_label: Option<Vec<String>>,
}

fn is_zero_i64(v: &i64) -> bool {
    *v == 0
}

#[derive(Serialize, Default)]
struct AdnMetaData {
    #[serde(skip_serializing_if = "String::is_empty")]
    usi: String,
}

#[derive(Serialize)]
struct AdnRequest {
    #[serde(rename = "adUnits")]
    ad_units: Vec<AdnRequestAdunit>,
    #[serde(rename = "metaData")]
    meta_data: AdnMetaData,
    #[serde(skip_serializing_if = "String::is_empty")]
    context: String,
    #[serde(rename = "kv", skip_serializing_if = "Option::is_none")]
    key_values: Option<serde_json::Value>,
}

/// `openrtb_ext.ImpExtAdnunitus`.
#[derive(Default)]
struct ImpExtAdnuntius {
    au_id: String,
    no_cookies: bool,
    max_deals: i64,
    network: String,
    bid_type: String,
    category: Option<Vec<String>>,
    segments: Option<Vec<String>>,
    keywords: Option<Vec<String>>,
    key_values: Option<BTreeMap<String, Vec<String>>>,
    ad_unit_matching_label: Option<Vec<String>>,
}

#[derive(Deserialize, Default)]
struct Targeting {
    #[serde(default)]
    c: Option<Vec<String>>,
    #[serde(default)]
    segments: Option<Vec<String>>,
    #[serde(default)]
    keywords: Option<Vec<String>>,
    #[serde(default)]
    kv: Option<BTreeMap<String, Vec<String>>>,
    #[serde(default)]
    auml: Option<Vec<String>>,
}

fn parse_adnuntius_ext(params: Option<&Val>) -> Result<ImpExtAdnuntius, BidderError> {
    const P: &str = "openrtb_ext.ImpExtAdnunitus";
    let mut out = ImpExtAdnuntius {
        au_id: str_field(params, "auId", &format!("{P}.Auid"))?,
        network: str_field(params, "network", &format!("{P}.Network"))?,
        bid_type: str_field(params, "bidType", &format!("{P}.BidType"))?,
        ..Default::default()
    };
    out.no_cookies = match field(params, "noCookies") {
        None => false,
        Some(v) if v.is_null() => false,
        Some(v) => v.as_bool().ok_or_else(|| {
            BidderError::FailedToUnmarshal(format!("cannot unmarshal {P}.NoCookies: expects true or false"))
        })?,
    };
    out.max_deals = int_field(params, "maxDeals")
        .map_err(|_| BidderError::FailedToUnmarshal(format!("cannot unmarshal {P}.MaxDeals")))?;
    if let Some(t) = field(params, "targeting").filter(|v| !v.is_null()) {
        let t: Targeting = sonic_rs::from_value(t)
            .map_err(|e| BidderError::FailedToUnmarshal(format!("cannot unmarshal {P}.Targeting: {e}")))?;
        out.category = t.c;
        out.segments = t.segments;
        out.keywords = t.keywords;
        out.key_values = t.kv;
        out.ad_unit_matching_label = t.auml;
    }
    Ok(out)
}

fn set_headers(request: &BidRequest) -> Header {
    let mut headers = Header::new();
    headers.add("Content-Type", "application/json;charset=utf-8");
    headers.add("Accept", "application/json");
    if let Some(d) = &request.device {
        if !d.ip.is_empty() {
            headers.add("X-Forwarded-For", d.ip.clone());
        }
        if !d.ua.is_empty() {
            headers.add("user-agent", d.ua.clone());
        }
    }
    headers
}

/// Go `getImpSizes`.
fn get_imp_sizes(imp: &Imp, bid_type: &str) -> Vec<Vec<i64>> {
    if bid_type == "banner" {
        if let Some(b) = &imp.banner {
            if !b.format.is_empty() {
                return b.format.iter().map(|f| vec![f.w, f.h]).collect();
            } else if let (Some(w), Some(h)) = (b.w, b.h) {
                return vec![vec![w, h]];
            }
        }
    }
    vec![]
}

/// Go `generateAdUnit`.
fn generate_ad_unit(imp: &Imp, ext: &ImpExtAdnuntius, bid_type: &str) -> AdnRequestAdunit {
    AdnRequestAdunit {
        au_id: ext.au_id.clone(),
        target_id: format!("{}-{}:{}", ext.au_id, imp.id, bid_type),
        ad_type: String::new(),
        native_request: NativeRequest::default(),
        dimensions: get_imp_sizes(imp, bid_type),
        max_deals: if ext.max_deals > 0 { ext.max_deals } else { 0 },
        category: ext.category.clone(),
        segments: ext.segments.clone(),
        keywords: ext.keywords.clone(),
        key_values: ext.key_values.clone(),
        ad_unit_matching_label: ext.ad_unit_matching_label.clone(),
    }
}

/// Go `getGDPR`: `(gdpr, consent)`.
fn get_gdpr(request: &BidRequest) -> Result<(String, String), BidderError> {
    let mut gdpr = String::new();
    if let Some(ext) = request.regs.as_ref().and_then(|r| r.ext.as_ref()) {
        let obj = obj_of(Some(&ext.0)).map_err(|e| {
            BidderError::other(format!("failed to parse ExtRegs in Adnuntius GDPR check: {e}"))
        })?;
        if let Some(v) = field(obj, "gdpr").filter(|v| !v.is_null()) {
            let g = v.as_i64().ok_or_else(|| {
                BidderError::other("failed to parse ExtRegs in Adnuntius GDPR check: cannot unmarshal openrtb_ext.ExtRegs.GDPR")
            })?;
            if g == 0 || g == 1 {
                gdpr = g.to_string();
            }
        }
    }
    let mut consent = String::new();
    if let Some(ext) = request.user.as_ref().and_then(|u| u.ext.as_ref()) {
        let obj = obj_of(Some(&ext.0)).map_err(|e| {
            BidderError::other(format!("failed to parse ExtUser in Adnuntius GDPR check: {e}"))
        })?;
        consent = str_field(obj, "consent", "openrtb_ext.ExtUser.Consent").map_err(|e| {
            BidderError::other(format!("failed to parse ExtUser in Adnuntius GDPR check: {e}"))
        })?;
    }
    Ok((gdpr, consent))
}

/// Go `url.Values.Encode` (sorted keys, `QueryEscape`).
fn encode_query(q: &BTreeMap<String, Vec<String>>) -> String {
    fn esc(s: &str) -> String {
        url::form_urlencoded::byte_serialize(s.as_bytes())
            .collect::<String>()
            .replace('*', "%2A")
            .replace("%7E", "~")
    }
    let mut parts = Vec::new();
    for (k, vs) in q {
        for v in vs {
            parts.push(format!("{}={}", esc(k), esc(v)));
        }
    }
    parts.join("&")
}

impl Adapter {
    /// Go `makeEndpointUrl`.
    fn make_endpoint_url(&self, request: &BidRequest, mut no_cookies: bool) -> Result<String, BidderError> {
        let (gdpr, consent) = get_gdpr(request)
            .map_err(|e| BidderError::other(format!("failed to parse GDPR information: {e}")))?;

        let base = if !gdpr.is_empty() { &self.extra_info } else { &self.endpoint };
        let (base_no_frag, _) = base.split_once('#').unwrap_or((base, ""));
        let (path, query) = base_no_frag.split_once('?').unwrap_or((base_no_frag, ""));
        let mut q: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (k, v) in url::form_urlencoded::parse(query.as_bytes()) {
            q.entry(k.into_owned()).or_default().push(v.into_owned());
        }

        if !no_cookies {
            if let Some(ext) = request.device.as_ref().and_then(|d| d.ext.as_ref()) {
                let obj = obj_of(Some(&ext.0))
                    .map_err(|e| BidderError::other(format!("failed to parse device ext: {e}")))?;
                if let Some(v) = field(obj, "noCookies").filter(|v| !v.is_null()) {
                    if v.as_bool() == Some(true) {
                        no_cookies = true;
                    } else if v.as_bool().is_none() {
                        return Err(BidderError::other("failed to parse device ext: cannot unmarshal adnuntius.extDeviceAdnuntius.NoCookies"));
                    }
                }
            }
        }

        let tzo = -self.tz_offset_seconds / MINUTES_IN_HOUR;
        if !gdpr.is_empty() {
            q.insert("gdpr".into(), vec![gdpr]);
        }
        if !consent.is_empty() {
            q.insert("consentString".into(), vec![consent]);
        }
        if no_cookies {
            q.insert("noCookies".into(), vec!["true".into()]);
        }
        q.insert("tzo".into(), vec![tzo.to_string()]);
        q.insert("format".into(), vec!["prebidServer".into()]);
        Ok(format!("{path}?{}", encode_query(&q)))
    }

    fn generate_requests(&self, request: &BidRequest) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut network_adunit_map: Vec<(String, Vec<AdnRequestAdunit>)> = Vec::new();
        let headers = set_headers(request);
        let mut no_cookies = false;

        for imp in &request.imp {
            let params = match imp_bidder_params(imp) {
                Ok(p) => p,
                Err((false, e)) => {
                    return (vec![], vec![BidderError::bad_input(format!("Error unmarshalling ExtImpBidder: {e}"))])
                }
                Err((true, e)) => {
                    return (vec![], vec![BidderError::bad_input(format!("Error unmarshalling ExtImpValues: {e}"))])
                }
            };
            let ext = match parse_adnuntius_ext(params) {
                Ok(e) => e,
                Err(e) => {
                    return (vec![], vec![BidderError::bad_input(format!("Error unmarshalling ExtImpValues: {e}"))])
                }
            };
            if ext.no_cookies {
                no_cookies = true;
            }
            let network =
                if ext.network.is_empty() { DEFAULT_NETWORK.to_string() } else { ext.network.clone() };

            if imp.video.is_some() {
                return (
                    vec![],
                    vec![BidderError::bad_input(format!(
                        "ignoring imp id={}, Adnuntius supports only native and banner",
                        imp.id
                    ))],
                );
            }

            let mut push = |unit: AdnRequestAdunit| {
                match network_adunit_map.iter_mut().find(|(n, _)| *n == network) {
                    Some((_, v)) => v.push(unit),
                    None => network_adunit_map.push((network.clone(), vec![unit])),
                }
            };
            if imp.banner.is_some() {
                let mut unit = generate_ad_unit(imp, &ext, "banner");
                unit.ad_type = String::new();
                push(unit);
            }
            if let Some(native) = &imp.native {
                let mut unit = generate_ad_unit(imp, &ext, "native");
                unit.ad_type = "NATIVE".to_string();
                match RawValue::from_string(native.request.clone()) {
                    Ok(raw) => unit.native_request.ortb = Some(raw),
                    Err(e) => {
                        return (
                            vec![],
                            vec![BidderError::bad_input(format!("Error unmarshalling Native: {e}"))],
                        )
                    }
                }
                push(unit);
            }
        }

        let endpoint = match self.make_endpoint_url(request, no_cookies) {
            Ok(e) => e,
            Err(e) => {
                return (vec![], vec![BidderError::bad_input(format!("failed to parse URL: [{e}]"))])
            }
        };

        let site = match request.site.as_ref().filter(|s| !s.page.is_empty()) {
            Some(s) => s.page.clone(),
            None => DEFAULT_SITE.to_string(),
        };

        // Go `getSiteExtAsKv`: `site.ext.data` as a generic value.
        let mut site_data: Option<serde_json::Value> = None;
        if let Some(ext) = request.site.as_ref().and_then(|s| s.ext.as_ref()) {
            match obj_of(Some(&ext.0)) {
                Err(e) => {
                    return (
                        vec![],
                        vec![BidderError::other(format!(
                            "failed to parse site Ext: [failed to parse site ext in Adnuntius: {e}]"
                        ))],
                    )
                }
                Ok(obj) => {
                    if let Some(d) = field(obj, "data").filter(|v| !v.is_null()) {
                        site_data = serde_json::from_str(&sonic_rs::to_string(d).unwrap_or_default()).ok();
                    }
                }
            }
        }

        // Go `jsonutil.Unmarshal(user.Ext, &extUser)`: first eid's first uid.
        let mut usi_from_eids = String::new();
        if let Some(ext) = request.user.as_ref().and_then(|u| u.ext.as_ref()) {
            match obj_of(Some(&ext.0)) {
                Err(e) => return (vec![], vec![BidderError::other(format!("failed to parse Ext User: {e}"))]),
                Ok(obj) => {
                    if let Some(first_uid) = field(obj, "eids")
                        .and_then(|e| e.as_array())
                        .and_then(|a| a.first())
                        .and_then(|e| e.get("uids"))
                        .and_then(|u| u.as_array())
                        .and_then(|a| a.first())
                    {
                        usi_from_eids = first_uid.get("id").and_then(|v| v.as_str()).unwrap_or_default().to_string();
                    }
                }
            }
        }

        let imp_ids: Vec<String> = request.imp.iter().map(|i| i.id.clone()).collect();
        let mut request_data = Vec::new();
        for (_, ad_units) in network_adunit_map {
            let mut adn = AdnRequest {
                ad_units,
                meta_data: AdnMetaData { usi: usi_from_eids.clone() },
                context: site.clone(),
                key_values: site_data.clone(),
            };
            if let Some(u) = &request.user {
                if !u.id.is_empty() {
                    adn.meta_data.usi = u.id.clone();
                }
            }
            match crate::go_json::to_vec(&adn) {
                Ok(body) => request_data.push(RequestData {
                    method: "POST".into(),
                    uri: endpoint.clone(),
                    body,
                    headers: headers.clone(),
                    imp_ids: imp_ids.clone(),
                }),
                Err(e) => {
                    return (
                        vec![],
                        vec![BidderError::bad_input(format!("Error unmarshalling adnuntius request: {e}"))],
                    )
                }
            }
        }
        (request_data, vec![])
    }
}

// ---- response types ---------------------------------------------------------------------------

#[derive(Deserialize, Default)]
struct AmountCurrency {
    #[serde(default, alias = "Amount")]
    amount: f64,
    #[serde(default, alias = "Currency", deserialize_with = "crate::ortb::de::string")]
    currency: String,
}

#[derive(Deserialize, Default)]
struct AdnAdvertiser {
    #[serde(rename = "legalName", default, alias = "LegalName")]
    legal_name: String,
    #[serde(default, alias = "Name")]
    name: String,
}

#[derive(Deserialize, Default)]
struct Ad {
    #[serde(default, alias = "Bid")]
    bid: AmountCurrency,
    #[serde(rename = "netBid", default, alias = "NetBid")]
    net_bid: AmountCurrency,
    #[serde(rename = "grossBid", default, alias = "GrossBid")]
    gross_bid: AmountCurrency,
    #[serde(rename = "dealId", default, deserialize_with = "crate::ortb::de::string")]
    deal_id: String,
    #[serde(rename = "adId", default, alias = "AdId", deserialize_with = "crate::ortb::de::string")]
    ad_id: String,
    #[serde(rename = "creativeWidth", default, alias = "CreativeWidth")]
    creative_width: Option<serde_json::Value>,
    #[serde(rename = "creativeHeight", default, alias = "CreativeHeight")]
    creative_height: Option<serde_json::Value>,
    #[serde(rename = "creativeId", default, alias = "CreativeId", deserialize_with = "crate::ortb::de::string")]
    creative_id: String,
    #[serde(rename = "lineItemId", default, alias = "LineItemId", deserialize_with = "crate::ortb::de::string")]
    line_item_id: String,
    #[serde(default, alias = "Html", deserialize_with = "crate::ortb::de::string")]
    html: String,
    #[serde(rename = "advertiserDomains", default, alias = "AdvertiserDomains", deserialize_with = "crate::ortb::de::strings")]
    advertiser_domains: Vec<String>,
    #[serde(default)]
    advertiser: AdnAdvertiser,
}

#[derive(Deserialize, Default)]
struct AdUnit {
    #[serde(rename = "targetId", default, alias = "TargetId", deserialize_with = "crate::ortb::de::string")]
    target_id: String,
    #[serde(default, alias = "Html", deserialize_with = "crate::ortb::de::string")]
    html: String,
    #[serde(rename = "matchedAdCount", default, alias = "MatchedAdCount")]
    matched_ad_count: i64,
    #[serde(rename = "nativeJson", default)]
    native_json: Option<Box<RawValue>>,
    #[serde(default, alias = "Ads", deserialize_with = "crate::ortb::de::seq")]
    ads: Vec<Ad>,
    #[serde(default, alias = "Deals", deserialize_with = "crate::ortb::de::seq")]
    deals: Vec<Ad>,
}

#[derive(Deserialize, Default)]
struct AdnResponse {
    #[serde(rename = "adUnits", default, alias = "AdUnits", deserialize_with = "crate::ortb::de::seq")]
    ad_units: Vec<AdUnit>,
}

/// jsoniter's message for a number where a string is expected.
fn check_string_field(v: &Option<serde_json::Value>, path: &str) -> Result<(), BidderError> {
    match v {
        None | Some(serde_json::Value::Null) | Some(serde_json::Value::String(_)) => Ok(()),
        Some(other) => {
            let c = other.to_string().chars().next().unwrap_or('\0');
            Err(BidderError::FailedToUnmarshal(format!(
                "cannot unmarshal {path}: expects \" or n, but found {c}"
            )))
        }
    }
}

fn check_ad(ad: &Ad) -> Result<(), BidderError> {
    check_string_field(&ad.creative_width, "adnuntius.Ad.CreativeWidth")?;
    check_string_field(&ad.creative_height, "adnuntius.Ad.CreativeHeight")
}

fn value_str(v: &Option<serde_json::Value>) -> String {
    match v {
        Some(serde_json::Value::String(s)) => s.clone(),
        _ => String::new(),
    }
}

fn convert_markup_type_to_bid_type(m: MarkupType) -> BidType {
    match m {
        MarkupType::BANNER => BidType::Banner,
        MarkupType::NATIVE => BidType::Native,
        _ => BidType::Banner,
    }
}

/// Go `generateReturnExt`.
fn generate_return_ext(ad: &Ad, request: &BidRequest) -> Result<Option<Ext>, BidderError> {
    let mut dsa_present = false;
    if let Some(ext) = request.regs.as_ref().and_then(|r| r.ext.as_ref()) {
        let obj = obj_of(Some(&ext.0))
            .map_err(|e| BidderError::other(format!("Failed to parse Ext information in Adnuntius: {e}")))?;
        dsa_present = field(obj, "dsa").is_some_and(|v| !v.is_null());
    }
    if !ad.advertiser.name.is_empty() && dsa_present {
        let legal_name = if ad.advertiser.legal_name.is_empty() {
            &ad.advertiser.name
        } else {
            &ad.advertiser.legal_name
        };
        #[derive(Serialize)]
        struct Dsa<'a> {
            adrender: i8,
            behalf: &'a str,
            paid: &'a str,
        }
        #[derive(Serialize)]
        struct ExtBid<'a> {
            dsa: Dsa<'a>,
        }
        let ext = Ext::from_serialize(&ExtBid { dsa: Dsa { adrender: 0, behalf: legal_name, paid: legal_name } })
            .map_err(|e| BidderError::other(format!("Failed to parse Ext information in Adnuntius: {e}")))?;
        return Ok(Some(ext));
    }
    Ok(None)
}

/// Go `generateAdResponse`.
fn generate_ad_response(
    ad: &Ad,
    imp: &Imp,
    html: String,
    m_type: MarkupType,
    request: &BidRequest,
) -> Result<Bid, BidderError> {
    let width_s = value_str(&ad.creative_width);
    let creative_width = width_s
        .parse::<i64>()
        .map_err(|_| BidderError::bad_server_response(format!("Value of width: {width_s} is not a string")))?;
    let height_s = value_str(&ad.creative_height);
    let creative_height = height_s
        .parse::<i64>()
        .map_err(|_| BidderError::bad_server_response(format!("Value of height: {height_s} is not a string")))?;

    let params = match imp_bidder_params(imp) {
        Ok(p) => p,
        Err((false, e)) => {
            return Err(BidderError::bad_server_response(format!("Error unmarshalling ExtImpBidder: {e}")))
        }
        Err((true, e)) => {
            return Err(BidderError::bad_server_response(format!("Error unmarshalling ExtImpValues: {e}")))
        }
    };
    let ext = parse_adnuntius_ext(params)
        .map_err(|e| BidderError::bad_server_response(format!("Error unmarshalling ExtImpValues: {e}")))?;

    let mut price = ad.bid.amount;
    if !ext.bid_type.is_empty() {
        if ext.bid_type.eq_ignore_ascii_case("net") {
            price = ad.net_bid.amount;
        }
        if ext.bid_type.eq_ignore_ascii_case("gross") {
            price = ad.gross_bid.amount;
        }
    }

    let ext_json = generate_return_ext(ad, request)
        .map_err(|e| BidderError::bad_server_response(format!("Error extracting Ext: {e}")))?;

    Ok(Bid {
        id: ad.ad_id.clone(),
        impid: imp.id.clone(),
        w: creative_width,
        h: creative_height,
        adid: ad.ad_id.clone(),
        dealid: ad.deal_id.clone(),
        cid: ad.line_item_id.clone(),
        crid: ad.creative_id.clone(),
        price: price * 1000.0,
        adm: html,
        mtype: m_type,
        adomain: ad.advertiser_domains.clone(),
        ext: ext_json,
        ..Default::default()
    })
}

/// Go `jsonparser.Get(data, "ortb")` on a raw JSON object.
fn raw_get_ortb(raw: &RawValue) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(raw.get()).ok()?;
    let obj = v.as_object()?;
    // Go returns the raw bytes of the member as written; re-slice them from the source text.
    obj.get("ortb")?;
    let rv: std::collections::BTreeMap<String, Box<RawValue>> = serde_json::from_str(raw.get()).ok()?;
    rv.get("ortb").map(|r| r.get().to_string())
}

fn generate_bid_response(
    adn_response: &AdnResponse,
    request: &BidRequest,
) -> (Option<BidderResponse>, Vec<BidderError>) {
    let mut bid_response = BidderResponse::with_bids_capacity(adn_response.ad_units.len());
    let mut currency = String::new();
    let mut adunit_map: std::collections::HashMap<String, &AdUnit> = std::collections::HashMap::new();
    let mut media_type_map: Vec<(String, Vec<&AdUnit>)> = Vec::new();

    for au in &adn_response.ad_units {
        let first = au.target_id.split(':').next().unwrap_or("").to_string();
        if au.matched_ad_count > 0 {
            match media_type_map.iter_mut().find(|(k, _)| *k == first) {
                Some((_, v)) => v.push(au),
                None => media_type_map.push((first, vec![au])),
            }
        }
    }
    for (target_id, mapped) in &media_type_map {
        let mut highest = 0usize;
        if mapped.len() > 1 {
            for index in 0..mapped.len() {
                // Go indexes `Ads[0]` and panics for an ad unit without ads; treat as amount 0.
                let amount = |i: usize| mapped[i].ads.first().map_or(0.0, |a| a.bid.amount);
                if amount(index) > amount(highest) {
                    highest = index;
                }
            }
        }
        adunit_map.insert(target_id.clone(), mapped[highest]);
    }

    for imp in &request.imp {
        let au_id = match imp.ext.as_ref().and_then(|e| e.0.get("bidder")).and_then(|b| b.get("auId")) {
            Some(v) => match v.as_str() {
                Some(s) => s.to_string(),
                None => sonic_rs::to_string(v).unwrap_or_default(),
            },
            None => {
                return (
                    None,
                    vec![BidderError::bad_input("Error at Bidder auId: Key path not found")],
                )
            }
        };
        let target_id = format!("{au_id}-{}", imp.id);
        let Some(adunit) = adunit_map.get(&target_id) else { continue };

        if let Some(ad) = adunit.ads.first() {
            let mut html = adunit.html.clone();
            let mut m_type = MarkupType::BANNER;
            let mut native: Option<String> = None;

            currency = ad.bid.currency.clone();
            if let Some(nj) = &adunit.native_json {
                match raw_get_ortb(nj) {
                    Some(n) => native = Some(n),
                    None => {
                        return (
                            None,
                            vec![BidderError::bad_server_response(format!(
                                "Failed to parse native json where imp id={}",
                                imp.id
                            ))],
                        )
                    }
                }
            }
            if let Some(n) = native {
                html = n;
                m_type = MarkupType::NATIVE;
            }

            let ad_bid = match generate_ad_response(ad, imp, html, m_type, request) {
                Ok(b) => b,
                Err(e) => return (None, vec![e]),
            };
            bid_response.bids.push(TypedBid::new(ad_bid, convert_markup_type_to_bid_type(m_type)));

            for deal in &adunit.deals {
                let m_type = MarkupType::BANNER;
                let deal_bid = match generate_ad_response(deal, imp, deal.html.clone(), m_type, request) {
                    Ok(b) => b,
                    Err(e) => return (None, vec![e]),
                };
                bid_response.bids.push(TypedBid::new(deal_bid, convert_markup_type_to_bid_type(m_type)));
            }
        }
    }
    bid_response.currency = currency;
    (Some(bid_response), vec![])
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        self.generate_requests(request)
    }

    fn make_bids(
        &self,
        request: &BidRequest,
        _request_data: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if response.status_code == 400 {
            return (
                None,
                vec![BidderError::bad_input(format!("Status code: {}, Request malformed", response.status_code))],
            );
        }
        if response.status_code != 200 {
            return (
                None,
                vec![BidderError::bad_server_response(format!(
                    "Status code: {}, Something went wrong with your request",
                    response.status_code
                ))],
            );
        }
        if response.body.iter().all(|b| b" \t\r\n".contains(b)) {
            return (None, vec![unmarshal_err('\0')]);
        }
        let adn_response: AdnResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        for au in &adn_response.ad_units {
            for ad in au.ads.iter().chain(au.deals.iter()) {
                if let Err(e) = check_ad(ad) {
                    return (None, vec![e]);
                }
            }
        }
        generate_bid_response(&adn_response, request)
    }
}

// ---- local helpers (Go `jsonutil.Unmarshal` into `json.RawMessage`-backed params) ----------
type Val = sonic_rs::Value;

fn unmarshal_err(first: char) -> BidderError {
    BidderError::FailedToUnmarshal(format!("expect {{ or n, but found {first}"))
}

/// Go `jsonutil.Unmarshal(raw, &struct)` first-byte check: absent input fails on the NUL byte,
/// anything but an object or `null` fails on its first character. `Ok(None)` is `null`.
fn obj_of(v: Option<&Val>) -> Result<Option<&Val>, BidderError> {
    let Some(v) = v else {
        return Err(unmarshal_err('\0'));
    };
    if v.is_object() {
        Ok(Some(v))
    } else if v.is_null() {
        Ok(None)
    } else {
        Err(unmarshal_err(sonic_rs::to_string(v).ok().and_then(|s| s.chars().next()).unwrap_or('\0')))
    }
}

/// jsoniter matches struct keys case-insensitively.
fn field<'a>(obj: Option<&'a Val>, name: &str) -> Option<&'a Val> {
    let obj = obj?.as_object()?;
    obj.iter().find(|(k, _)| *k == name).or_else(|| obj.iter().find(|(k, _)| k.eq_ignore_ascii_case(name))).map(|(_, v)| v)
}

/// A Go `string` field: absent or `null` is `""`, other types fail like jsoniter.
fn str_field(obj: Option<&Val>, name: &str, path: &str) -> Result<String, BidderError> {
    match field(obj, name) {
        None => Ok(String::new()),
        Some(v) if v.is_null() => Ok(String::new()),
        Some(v) => match v.as_str() {
            Some(s) => Ok(s.to_string()),
            None => {
                let c = sonic_rs::to_string(v).ok().and_then(|s| s.chars().next()).unwrap_or('\0');
                Err(BidderError::FailedToUnmarshal(format!(
                    "cannot unmarshal {path}: expects \" or n, but found {c}"
                )))
            }
        },
    }
}

/// `imp.ext` -> `{"bidder": ...}` -> the bidder params object (Go `ExtImpBidder`).
/// The error is `(failed_at_bidder_step, error)`.
fn imp_bidder_params(imp: &Imp) -> Result<Option<&Val>, (bool, BidderError)> {
    let outer = obj_of(imp.ext.as_ref().map(|e| &e.0)).map_err(|e| (false, e))?;
    obj_of(field(outer, "bidder")).map_err(|e| (true, e))
}

/// Go `template.Parse` rejects an action that is not a field (`{{Malformed}}`).
fn parse_template(endpoint: &str) -> Result<EndpointTemplate, String> {
    let mut rest = endpoint;
    while let Some(open) = rest.find("{{") {
        let after = &rest[open + 2..];
        if let Some(close) = after.find("}}") {
            let action = after[..close].trim();
            if !action.starts_with('.') {
                return Err(format!(
                    "unable to parse endpoint url template: template: endpointTemplate:1: function \"{action}\" not defined"
                ));
            }
            rest = &after[close + 2..];
        } else {
            break;
        }
    }
    EndpointTemplate::parse(endpoint)
        .map_err(|e| format!("unable to parse endpoint url template: {e}"))
}
// ---------------------------------------------------------------------------------------------

/// A Go `int`/`int64` field: absent or `null` is 0. `Err(())` for a non-integer value.
fn int_field(obj: Option<&Val>, name: &str) -> Result<i64, ()> {
    match field(obj, name) {
        None => Ok(0),
        Some(v) if v.is_null() => Ok(0),
        Some(v) => v.as_i64().ok_or(()),
    }
}

/// Go `jsonutil.Unmarshal(bid.Ext, &openrtb_ext.ExtBid)` then `bidExt.Prebid.Type`:
/// `None` when the ext is absent, fails to parse or has no `prebid` object; otherwise the type
/// text (`""` when `prebid.type` is missing).
fn prebid_type(ext: Option<&Ext>) -> Option<String> {
    let ext = ext?;
    let obj = obj_of(Some(&ext.0)).ok()??;
    let prebid = field(Some(obj), "prebid")?;
    if prebid.is_null() {
        return None;
    }
    if !prebid.is_object() {
        return None;
    }
    match field(Some(prebid), "type") {
        None => Some(String::new()),
        Some(v) if v.is_null() => Some(String::new()),
        Some(v) => v.as_str().map(str::to_string),
    }
}

/// Go `openrtb_ext.ParseBidType`.
fn parse_bid_type(s: &str) -> Result<BidType, BidderError> {
    BidType::parse(s).map_err(BidderError::other)
}
