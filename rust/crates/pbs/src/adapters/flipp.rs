//! Go `adapters/flipp/flipp.go`.
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
use serde::{Deserialize, Serialize};
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};

const BANNER_TYPE: BidType = BidType::Banner;
const INLINE_DIV_NAME: &str = "inline";
const DEFAULT_CURRENCY: &str = "USD";
const DEFAULT_STANDARD_HEIGHT: i64 = 2400;
const DEFAULT_COMPACT_HEIGHT: i64 = 600;
const COUNT: i64 = 1;
const AD_TYPES: [i64; 2] = [4309, 641];
const DTX_TYPES: [i64; 1] = [5061];

type UuidGenerator = Box<dyn Fn() -> Result<String, String> + Send + Sync>;

pub struct Adapter {
    endpoint: String,
    uuid_generator: UuidGenerator,
}

impl Adapter {
    /// Go `Builder` (random UUID generator).
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self { endpoint: endpoint.into(), uuid_generator: Box::new(random_uuid) }
    }

    /// Go tests swap `uuidGenerator`.
    pub fn with_uuid_generator(
        endpoint: impl Into<String>,
        generator: impl Fn() -> Result<String, String> + Send + Sync + 'static,
    ) -> Self {
        Self { endpoint: endpoint.into(), uuid_generator: Box::new(generator) }
    }
}

fn random_uuid() -> Result<String, String> {
    let mut bytes = [0u8; 16];
    for chunk in bytes.chunks_mut(8) {
        let mut h = RandomState::new().build_hasher();
        h.write_u8(0);
        chunk.copy_from_slice(&h.finish().to_le_bytes());
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    Ok(format!("{}-{}-{}-{}-{}", &hex[0..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..32]))
}

#[derive(Default, Clone, Serialize)]
struct FlippOptions {
    #[serde(rename = "startCompact", skip_serializing_if = "std::ops::Not::not")]
    start_compact: bool,
    #[serde(rename = "dwellExpand", skip_serializing_if = "std::ops::Not::not")]
    dwell_expand: bool,
    #[serde(rename = "contentCode", skip_serializing_if = "String::is_empty")]
    content_code: String,
}

#[derive(Default)]
struct ImpExtFlipp {
    publisher_name_identifier: String,
    creative_type: String,
    site_id: i64,
    zone_ids: Vec<i64>,
    user_key: String,
    options: FlippOptions,
}

fn bool_field(obj: Option<&Val>, name: &str) -> Result<bool, BidderError> {
    match field(obj, name) {
        None => Ok(false),
        Some(v) if v.is_null() => Ok(false),
        Some(v) => v.as_bool().ok_or_else(|| {
            BidderError::FailedToUnmarshal(format!("cannot unmarshal openrtb_ext.ImpExtFlippOptions.{name}"))
        }),
    }
}

/// Go `jsonparser.Get(imp.Ext, "bidder")` then `jsonutil.Unmarshal(params, &ImpExtFlipp)`.
fn parse_flipp_params(imp: &Imp) -> Result<ImpExtFlipp, BidderError> {
    let bidder = imp
        .ext
        .as_ref()
        .and_then(|e| e.0.get("bidder"))
        .ok_or_else(|| BidderError::other("flipp params not found. Key path not found"))?;
    let params = obj_of(Some(bidder)).map_err(|e| BidderError::other(format!("unable to extract flipp params. {e}")))?;
    let wrap = |e: BidderError| BidderError::other(format!("unable to extract flipp params. {e}"));
    let mut out = ImpExtFlipp {
        publisher_name_identifier: str_field(params, "publisherNameIdentifier", "openrtb_ext.ImpExtFlipp.PublisherNameIdentifier").map_err(wrap)?,
        creative_type: str_field(params, "creativeType", "openrtb_ext.ImpExtFlipp.CreativeType").map_err(wrap)?,
        site_id: int_field(params, "siteId").map_err(|_| wrap(BidderError::FailedToUnmarshal("cannot unmarshal openrtb_ext.ImpExtFlipp.SiteID".into())))?,
        user_key: str_field(params, "userKey", "openrtb_ext.ImpExtFlipp.UserKey").map_err(wrap)?,
        ..Default::default()
    };
    if let Some(z) = field(params, "zoneIds").filter(|v| !v.is_null()) {
        let arr = z.as_array().ok_or_else(|| wrap(BidderError::FailedToUnmarshal("cannot unmarshal openrtb_ext.ImpExtFlipp.ZoneIds".into())))?;
        for x in arr.iter() {
            out.zone_ids.push(x.as_i64().ok_or_else(|| wrap(BidderError::FailedToUnmarshal("cannot unmarshal openrtb_ext.ImpExtFlipp.ZoneIds".into())))?);
        }
    }
    if let Some(o) = field(params, "options").filter(|v| !v.is_null()) {
        let o = obj_of(Some(o)).map_err(wrap)?;
        out.options = FlippOptions {
            start_compact: bool_field(o, "startCompact").map_err(wrap)?,
            dwell_expand: bool_field(o, "dwellExpand").map_err(wrap)?,
            content_code: str_field(o, "contentCode", "openrtb_ext.ImpExtFlippOptions.ContentCode").map_err(wrap)?,
        };
    }
    Ok(out)
}

#[derive(Serialize)]
struct CampaignRequestBodyUser<'a> {
    key: &'a str,
}

#[derive(Serialize)]
struct Properties<'a> {
    #[serde(rename = "contentCode")]
    content_code: &'a str,
}

#[derive(Serialize)]
struct PrebidRequest<'a> {
    #[serde(rename = "creativeType")]
    creative_type: &'a str,
    height: i64,
    #[serde(rename = "publisherNameIdentifier")]
    publisher_name_identifier: &'a str,
    #[serde(rename = "requestId")]
    request_id: &'a str,
    width: i64,
}

#[derive(Serialize)]
struct Placement<'a> {
    #[serde(rename = "adTypes")]
    ad_types: &'a [i64],
    count: i64,
    #[serde(rename = "divName")]
    div_name: &'a str,
    prebid: PrebidRequest<'a>,
    properties: Properties<'a>,
    #[serde(rename = "siteId")]
    site_id: i64,
    #[serde(rename = "zoneIds")]
    zone_ids: &'a [i64],
    options: &'a FlippOptions,
}

#[derive(Serialize)]
struct CampaignRequestBody<'a> {
    #[serde(skip_serializing_if = "str::is_empty")]
    ip: &'a str,
    keywords: Vec<&'a str>,
    placements: Vec<Placement<'a>>,
    #[serde(skip_serializing_if = "str::is_empty")]
    url: &'a str,
    user: CampaignRequestBodyUser<'a>,
}

#[derive(Deserialize, Default)]
struct CampaignResponseBody {
    #[serde(default)]
    decisions: Option<Decisions>,
}

#[derive(Deserialize, Default)]
struct Decisions {
    #[serde(default)]
    inline: Option<Vec<Option<InlineModel>>>,
}

#[derive(Deserialize, Default)]
struct InlineModel {
    #[serde(rename = "adId", default)]
    ad_id: i64,
    #[serde(default)]
    contents: Vec<Option<Content>>,
    #[serde(rename = "creativeId", default)]
    creative_id: i64,
    #[serde(default)]
    prebid: Option<PrebidResponse>,
}

#[derive(Deserialize, Default)]
struct Content {
    #[serde(default)]
    data: Option<Data2>,
}

#[derive(Deserialize, Default)]
struct Data2 {
    #[serde(rename = "customData", default)]
    custom_data: Option<serde_json::Value>,
    #[serde(default)]
    width: i64,
}

#[derive(Deserialize, Default)]
struct PrebidResponse {
    #[serde(default)]
    cpm: Option<f64>,
    #[serde(default)]
    creative: Option<String>,
    #[serde(rename = "requestId", default)]
    request_id: Option<String>,
}

/// Go `url.Query().Get("flipp-content-code")` on the page URL.
fn query_get(page: &str, key: &str) -> String {
    let no_frag = page.split('#').next().unwrap_or("");
    let Some((_, q)) = no_frag.split_once('?') else { return String::new() };
    url::form_urlencoded::parse(q.as_bytes())
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.into_owned())
        .unwrap_or_default()
}

/// Go `paramsUserKeyPermitted`.
fn params_user_key_permitted(request: &BidRequest) -> bool {
    if let Some(regs) = &request.regs {
        if regs.coppa == 1 {
            return false;
        }
        if regs.gdpr == Some(1) {
            return false;
        }
    }
    if let Some(ext) = &request.ext {
        if let Ok(Some(obj)) = obj_of(Some(&ext.0)) {
            if let Some(v) = field(Some(obj), "transmitEids") {
                if v.as_bool() == Some(false) {
                    return false;
                }
            }
        }
    }
    if let Some(user) = &request.user {
        if !user.consent.is_empty() {
            use base64::Engine as _;
            let Ok(data) = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(user.consent.as_bytes()) else {
                return true;
            };
            let Some(allowed) = tcf1_purpose_allowed(&data, 4) else {
                return true;
            };
            if !allowed {
                return false;
            }
        }
    }
    true
}

// ---- go-gdpr `vendorconsent/tcf1`: `Parse` then `PurposeAllowed` -----------------------------

fn is_set(data: &[u8], bit: usize) -> bool {
    data.get(bit / 8).is_some_and(|b| b & (0x80 >> (bit % 8)) != 0)
}

fn parse_u16_bits(data: &[u8], bit: usize) -> Option<u16> {
    let start = bit / 8;
    let off = bit % 8;
    if off == 0 {
        if data.len() < start + 2 {
            return None;
        }
        return Some(u16::from_be_bytes([data[start], data[start + 1]]));
    }
    if data.len() < start + 3 {
        return None;
    }
    let comp = 8 - off;
    let left = ((data[start] & (0xffu8 >> off)) << off) | (data[start + 1] >> comp);
    let right = ((data[start + 2] & (0xffu8 << comp)) >> comp) | (data[start + 1] << off);
    Some(u16::from_be_bytes([left, right]))
}

/// `None` when `tcf1.Parse` fails; else `PurposeAllowed(purpose)`.
fn tcf1_purpose_allowed(data: &[u8], purpose: usize) -> Option<bool> {
    if data.len() < 22 {
        return None;
    }
    let max_vendor = u16::from_be_bytes([((data[19] & 0x0f) << 4) + ((data[20] & 0xf0) >> 4), ((data[20] & 0x0f) << 4) + ((data[21] & 0xf0) >> 4)]);
    if max_vendor < 1 {
        return None;
    }
    if (data[0] >> 2) < 1 {
        return None;
    }
    let right = ((data[16] & 0xf0) >> 4) | ((data[15] & 0x0f) << 4);
    let left = data[15] >> 4;
    if u16::from_be_bytes([left, right]) == 0 {
        return None;
    }
    if is_set(data, 172) {
        // range section
        if data.len() < 24 {
            return None;
        }
        let left = ((data[21] & 0x03) << 2) | (data[22] >> 6);
        let right = (data[22] << 2) | (data[23] >> 6);
        let num = u16::from_be_bytes([left, right]);
        let mut offset = 186usize;
        for _ in 0..num {
            if data.len() <= offset / 8 {
                return None;
            }
            if is_set(data, offset) {
                let start = parse_u16_bits(data, offset + 1)?;
                let end = parse_u16_bits(data, offset + 17)?;
                if start == 0 || end > max_vendor || end <= start {
                    return None;
                }
                offset += 33;
            } else {
                let id = parse_u16_bits(data, offset + 1)?;
                if id == 0 || id > max_vendor {
                    return None;
                }
                offset += 17;
            }
        }
    } else {
        // bit field
        if max_vendor > 3 {
            let mut other = (max_vendor as usize - 3) / 8;
            if (max_vendor as usize - 3) % 8 > 0 {
                other += 1;
            }
            if data.len() < 22 + other {
                return None;
            }
        }
    }
    Some(is_set(data, purpose + 131))
}

impl Adapter {
    fn process_imp(&self, request: &BidRequest, imp: &Imp) -> Result<RequestData, BidderError> {
        let flipp = parse_flipp_params(imp)?;

        // Go dereferences `request.Site` and panics when it is nil; report an error instead.
        let site = request
            .site
            .as_ref()
            .ok_or_else(|| BidderError::other("unable to parse site url. site is required"))?;
        // Go `url.Parse` only fails on malformed escapes or control characters here.
        if site.page.bytes().any(|b| b < 0x20 || b == 0x7f) {
            return Err(BidderError::other("unable to parse site url. net/url: invalid control character in URL"));
        }

        let content_code = if !flipp.options.content_code.is_empty() {
            flipp.options.content_code.clone()
        } else {
            query_get(&site.page, "flipp-content-code")
        };

        let ad_types: &[i64] = if flipp.creative_type == "DTX" { &DTX_TYPES } else { &AD_TYPES };

        let (mut height, mut width) = (0i64, 0i64);
        if let Some(b) = imp.banner.as_ref() {
            if let Some(f) = b.format.first() {
                height = f.h;
                width = f.w;
            }
        }

        let user_ip = match request.device.as_ref().filter(|d| !d.ip.is_empty()) {
            Some(d) => d.ip.clone(),
            None => return Err(BidderError::other("no IP set in flipp bidder params or request device")),
        };

        let user_key = if let Some(u) = request.user.as_ref().filter(|u| !u.id.is_empty()) {
            u.id.clone()
        } else if !flipp.user_key.is_empty() && params_user_key_permitted(request) {
            flipp.user_key.clone()
        } else {
            (self.uuid_generator)()
                .map_err(|e| BidderError::other(format!("unable to generate user uuid. {e}")))?
        };

        let keywords: Vec<&str> = site.keywords.split(',').collect();
        let placement = Placement {
            ad_types,
            count: COUNT,
            div_name: INLINE_DIV_NAME,
            prebid: PrebidRequest {
                creative_type: &flipp.creative_type,
                height,
                publisher_name_identifier: &flipp.publisher_name_identifier,
                request_id: &imp.id,
                width,
            },
            properties: Properties { content_code: &content_code },
            site_id: flipp.site_id,
            zone_ids: &flipp.zone_ids,
            options: &flipp.options,
        };
        let body = CampaignRequestBody {
            ip: &user_ip,
            keywords,
            placements: vec![placement],
            url: &site.page,
            user: CampaignRequestBodyUser { key: &user_key },
        };
        let body = crate::go_json::to_vec(&body)
            .map_err(|e| BidderError::other(format!("make request failed with err {e}")))?;

        let mut headers = Header::new();
        headers.add("Content-Type", "application/json");
        if let Some(d) = request.device.as_ref().filter(|d| !d.ua.is_empty()) {
            headers.add("User-Agent", d.ua.clone());
        }
        Ok(RequestData {
            method: "POST".into(),
            uri: self.endpoint.clone(),
            body,
            headers,
            imp_ids: vec![imp.id.clone()],
        })
    }
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut adapter_requests = Vec::with_capacity(request.imp.len());
        let mut errors = Vec::new();
        for imp in &request.imp {
            match self.process_imp(request, imp) {
                Ok(r) => adapter_requests.push(r),
                Err(e) => errors.push(e),
            }
        }
        if adapter_requests.is_empty() {
            errors.push(BidderError::other("adapterRequest is empty"));
            return (vec![], errors);
        }
        (adapter_requests, errors)
    }

    fn make_bids(
        &self,
        request: &BidRequest,
        _request_data: &RequestData,
        response_data: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if is_response_status_code_no_content(response_data) {
            return (None, vec![]);
        }
        if let Some(err) = check_response_status_code_for_errors(response_data) {
            return (None, vec![err]);
        }
        let campaign: CampaignResponseBody = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };

        let mut bid_response = BidderResponse::with_bids_capacity(request.imp.len());
        bid_response.currency = DEFAULT_CURRENCY.to_string();
        // Go keeps `flippExtParams` in a package variable; here it lives for one call.
        for imp in &request.imp {
            let start_compact = match parse_flipp_params(imp) {
                Ok(p) => p.options.start_compact,
                Err(e) => return (None, vec![e]),
            };
            // Go dereferences `Decisions` and each pointer below; a nil would panic.
            let Some(decisions) = campaign.decisions.as_ref() else {
                return (None, vec![BidderError::bad_server_response("decisions missing in the response")]);
            };
            for decision in decisions.inline.iter().flatten() {
                let Some(decision) = decision else {
                    return (None, vec![BidderError::bad_server_response("nil decision in the response")]);
                };
                let Some(prebid) = decision.prebid.as_ref() else {
                    return (None, vec![BidderError::bad_server_response("decision.prebid missing in the response")]);
                };
                let Some(request_id) = prebid.request_id.as_ref() else {
                    return (None, vec![BidderError::bad_server_response("decision.prebid.requestId missing in the response")]);
                };
                if *request_id == imp.id {
                    match build_bid(decision, &imp.id, start_compact) {
                        Ok(bid) => bid_response.bids.push(TypedBid::new(bid, BANNER_TYPE)),
                        Err(e) => return (None, vec![e]),
                    }
                }
            }
        }
        (Some(bid_response), vec![])
    }
}

/// Go `buildBid`.
fn build_bid(decision: &InlineModel, imp_id: &str, start_compact: bool) -> Result<Bid, BidderError> {
    let missing = |what: &str| BidderError::bad_server_response(format!("{what} missing in the response"));
    let prebid = decision.prebid.as_ref().ok_or_else(|| missing("decision.prebid"))?;
    let mut bid = Bid {
        crid: decision.creative_id.to_string(),
        price: prebid.cpm.ok_or_else(|| missing("decision.prebid.cpm"))?,
        adm: prebid.creative.clone().ok_or_else(|| missing("decision.prebid.creative"))?,
        id: decision.ad_id.to_string(),
        impid: imp_id.to_string(),
        ..Default::default()
    };
    // Go: `len(Contents) > 0 || Contents[0] != nil || ...` panics on an empty slice, and
    // `Contents[0].Data` is dereferenced next.
    let content = decision
        .contents
        .first()
        .and_then(|c| c.as_ref())
        .ok_or_else(|| missing("decision.contents[0]"))?;
    let data = content.data.as_ref().ok_or_else(|| missing("decision.contents[0].data"))?;
    if data.width != 0 {
        bid.w = data.width;
    }
    bid.h = if start_compact { DEFAULT_COMPACT_HEIGHT } else { DEFAULT_STANDARD_HEIGHT };
    if let Some(serde_json::Value::Object(map)) = &data.custom_data {
        let key = if start_compact { "compactHeight" } else { "standardHeight" };
        if let Some(serde_json::Value::Number(n)) = map.get(key) {
            if let Some(f) = n.as_f64() {
                bid.h = f as i64;
            }
        }
    }
    Ok(bid)
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
