//! Go `adapters/invibes/invibes.go`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::bid_types::BidType;
use crate::bidder::{Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::macros::{EndpointTemplate, EndpointTemplateParams};
use crate::ortb::openrtb2::{Banner, Bid, BidRequest, Format};
use crate::ortb::Ext;

const ADAPTER_VERSION: &str = "prebid_1.0.0";
const INVIBES_BID_VERSION: &str = "4";

pub struct Adapter {
    endpoint_template: EndpointTemplate,
}

#[derive(Serialize)]
struct InvibesAdRequest {
    #[serde(rename = "BidParamsJson")]
    bid_params_json: String,
    #[serde(rename = "Location")]
    location: String,
    #[serde(rename = "Lid")]
    lid: String,
    #[serde(rename = "IsTestBid")]
    is_test_bid: bool,
    #[serde(rename = "Kw")]
    kw: String,
    #[serde(rename = "IsAmp")]
    is_amp: bool,
    #[serde(rename = "Width")]
    width: String,
    #[serde(rename = "Height")]
    height: String,
    #[serde(rename = "GdprConsent")]
    gdpr_consent: String,
    #[serde(rename = "Gdpr")]
    gdpr: bool,
    #[serde(rename = "Bvid")]
    bvid: String,
    #[serde(rename = "InvibBVLog")]
    invib_bv_log: bool,
    #[serde(rename = "VideoAdDebug")]
    video_ad_debug: bool,
}

#[derive(Serialize, Default)]
struct InvibesBidParams {
    #[serde(rename = "PlacementIds")]
    placement_ids: Option<Vec<String>>,
    #[serde(rename = "BidVersion")]
    bid_version: String,
    #[serde(rename = "Properties")]
    properties: BTreeMap<String, InvibesPlacementProperty>,
}

#[derive(Serialize, Clone)]
struct InvibesPlacementProperty {
    #[serde(rename = "Formats")]
    formats: Option<Vec<Format>>,
    #[serde(rename = "ImpId")]
    imp_id: String,
}

#[derive(Default)]
struct InvibesInternalParams {
    bid_params: InvibesBidParams,
    domain_id: i64,
    is_amp: bool,
    gdpr: bool,
    gdpr_consent: String,
    test_bvid: String,
    test_log: bool,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct BidServerBidderResponse {
    currency: String,
    #[serde(rename = "typedbids")]
    typed_bids: Vec<BidServerTypedBid>,
    error: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct BidServerTypedBid {
    bid: Bid,
    #[serde(rename = "dealpriority")]
    deal_priority: i64,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpInvibes {
    #[serde(rename = "placementid")]
    placement_id: String,
    #[serde(rename = "domainid")]
    domain_id: i64,
    debug: ExtImpInvibesDebug,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpInvibesDebug {
    #[serde(rename = "testbvid")]
    test_bvid: String,
    #[serde(rename = "testlog")]
    test_log: bool,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Result<Self, BidderError> {
        let endpoint = endpoint.into();
        let wrap = |e: String| BidderError::other(format!("unable to parse endpoint url template: {e}"));
        check_template(&endpoint).map_err(wrap)?;
        let t = EndpointTemplate::parse(&endpoint).map_err(wrap)?;
        Ok(Self { endpoint_template: t })
    }

    fn make_url(&self, domain_id: i64) -> Result<String, BidderError> {
        let subdomain = if domain_id == 0 || domain_id == 1 || domain_id == 1001 {
            "bid".to_string()
        } else if domain_id < 1002 {
            format!("bid{domain_id}")
        } else {
            format!("bid{}", domain_id - 1000)
        };
        let params = EndpointTemplateParams { zone_id: subdomain, ..Default::default() };
        let host = self
            .endpoint_template
            .resolve(&params)
            .map_err(|e| BidderError::bad_input(format!("Unable to parse url template: {e}")))?;
        url::Url::parse(&host)
            .map_err(|e| BidderError::bad_input(format!("Unable to parse url template: parse \"{host}\": {e}")))?;
        Ok(host)
    }

    fn make_parameter(&self, p: &InvibesInternalParams, request: &BidRequest) -> Result<InvibesAdRequest, BidderError> {
        let lid = match &request.user {
            Some(u) if !u.buyeruid.is_empty() => u.buyeruid.clone(),
            _ => String::new(),
        };
        let Some(site) = &request.site else {
            return Err(BidderError::bad_input("Site not specified"));
        };
        let (mut width, mut height) = (String::new(), String::new());
        if let Some(d) = &request.device {
            if d.w > 0 {
                width = d.w.to_string();
            }
            if d.h > 0 {
                height = d.h.to_string();
            }
        }
        let bid_params_json = crate::go_json::to_vec(&p.bid_params).map_err(|e| BidderError::other(e.to_string()))?;
        Ok(InvibesAdRequest {
            is_test_bid: !p.test_bvid.is_empty(),
            bid_params_json: String::from_utf8_lossy(&bid_params_json).into_owned(),
            location: site.page.clone(),
            lid,
            kw: site.keywords.clone(),
            is_amp: p.is_amp,
            width,
            height,
            gdpr_consent: p.gdpr_consent.clone(),
            gdpr: p.gdpr,
            bvid: p.test_bvid.clone(),
            invib_bv_log: p.test_log,
            video_ad_debug: p.test_log,
        })
    }

    fn make_request(&self, p: &InvibesInternalParams, request: &BidRequest) -> Result<RequestData, BidderError> {
        let url = self.make_url(p.domain_id)?;
        let parameter = self.make_parameter(p, request)?;
        let body = crate::go_json::to_vec(&parameter).map_err(|e| BidderError::other(e.to_string()))?;

        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        if let Some(d) = &request.device {
            headers.add("User-Agent", d.ua.clone());
            if !d.ip.is_empty() {
                headers.add("X-Forwarded-For", d.ip.clone());
            } else if !d.ipv6.is_empty() {
                headers.add("X-Forwarded-For", d.ipv6.clone());
            }
        }
        if let Some(site) = &request.site {
            headers.add("Referer", site.page.clone());
        }
        headers.add("Aver", ADAPTER_VERSION);

        Ok(RequestData {
            method: "POST".into(),
            uri: url,
            headers,
            body,
            // Go iterates a map here (random order); the Properties map order is used.
            imp_ids: p.bid_params.properties.values().map(|v| v.imp_id.clone()).collect(),
        })
    }
}

fn read_gdpr(request: &BidRequest) -> (bool, String) {
    let mut consent = String::new();
    if let Some(user) = &request.user {
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct ExtUser {
            consent: String,
        }
        if let Ok(u) = unmarshal_ext::<ExtUser>(user.ext.as_ref()) {
            consent = u.consent;
        }
    }
    let mut gdpr_applies = true;
    if let Some(regs) = &request.regs {
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct ExtRegs {
            gdpr: Option<i8>,
        }
        if let Ok(r) = unmarshal_ext::<ExtRegs>(regs.ext.as_ref()) {
            if let Some(g) = r.gdpr {
                gdpr_applies = g == 1;
            }
        }
    }
    (gdpr_applies, consent)
}

fn read_ad_formats(banner: &Banner) -> Option<Vec<Format>> {
    if !banner.format.is_empty() {
        Some(banner.format.clone())
    } else if let (Some(w), Some(h)) = (banner.w, banner.h) {
        Some(vec![Format { w, h, ..Default::default() }])
    } else {
        None
    }
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut temp_errors = Vec::new();
        let (gdpr_applies, consent) = read_gdpr(request);

        let mut p = InvibesInternalParams {
            bid_params: InvibesBidParams {
                properties: BTreeMap::new(),
                bid_version: INVIBES_BID_VERSION.into(),
                placement_ids: None,
            },
            ..Default::default()
        };

        for imp in &request.imp {
            let bidder_ext: ExtImpBidder = match unmarshal_ext(imp.ext.as_ref()) {
                Ok(v) => v,
                Err(_) => {
                    temp_errors.push(BidderError::bad_input("Error parsing bidderExt object"));
                    continue;
                }
            };
            let invibes: ExtImpInvibes = match unmarshal_ext_deep(bidder_ext.bidder.as_ref()) {
                Ok(v) => v,
                Err(_) => {
                    temp_errors.push(BidderError::bad_input("Error parsing invibesExt parameters"));
                    continue;
                }
            };
            let Some(banner) = &imp.banner else {
                temp_errors.push(BidderError::bad_input("Banner not specified"));
                continue;
            };
            let ad_formats = read_ad_formats(banner);

            p.domain_id = invibes.domain_id;
            p.bid_params
                .placement_ids
                .get_or_insert_with(Vec::new)
                .push(invibes.placement_id.trim().to_string());
            p.bid_params.properties.insert(
                invibes.placement_id.clone(),
                InvibesPlacementProperty { imp_id: imp.id.clone(), formats: ad_formats },
            );
            if !invibes.debug.test_bvid.is_empty() {
                p.test_bvid = invibes.debug.test_bvid.clone();
            }
            p.test_log = invibes.debug.test_log;
        }
        if req_info.pbs_entry_point == "amp" {
            p.is_amp = true;
        }
        if p.bid_params.placement_ids.as_ref().map_or(true, |v| v.is_empty()) {
            return (vec![], temp_errors);
        }

        p.gdpr = gdpr_applies;
        p.gdpr_consent = consent;

        match self.make_request(&p, request) {
            Ok(r) => (vec![r], vec![]),
            Err(e) => (vec![], vec![e]),
        }
    }

    fn make_bids(
        &self,
        _internal_request: &BidRequest,
        _external_request: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if response.status_code != 200 {
            return (
                None,
                vec![BidderError::other(format!("Unexpected status code: {}.", response.status_code))],
            );
        }
        let bid_response: BidServerBidderResponse = match unmarshal_bytes(&response.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut parsed = BidderResponse::with_bids_capacity(bid_response.typed_bids.len());
        parsed.currency = bid_response.currency.clone();
        if !bid_response.error.is_empty() {
            return (None, vec![BidderError::other(format!("Server error: {}.", bid_response.error))]);
        }
        for tb in bid_response.typed_bids {
            let mut t = TypedBid::new(tb.bid, BidType::Banner);
            t.deal_priority = tb.deal_priority as i32;
            parsed.bids.push(t);
        }
        (Some(parsed), vec![])
    }
}

/// Go `jsonutil.Unmarshal` into a struct whose tagged keys are lower-cased here.
fn unmarshal_bytes<T: serde::de::DeserializeOwned + Default>(body: &[u8]) -> Result<T, BidderError> {
    let first = body.iter().find(|b| !b" \t\r\n".contains(b)).copied();
    match first {
        Some(b'{') => {}
        Some(b'n') => return Ok(T::default()),
        Some(c) => return Err(BidderError::FailedToUnmarshal(format!("expect {{ or n, but found {}", c as char))),
        None => return jsonutil::unmarshal(body),
    }
    let map: serde_json::Map<String, serde_json::Value> = jsonutil::unmarshal_any(body)?;
    let lowered: serde_json::Map<String, serde_json::Value> =
        map.into_iter().map(|(k, v)| (k.to_lowercase(), v)).collect();
    serde_json::from_value(serde_json::Value::Object(lowered))
        .map_err(|e| BidderError::FailedToUnmarshal(e.to_string()))
}

fn lower_deep(v: serde_json::Value) -> serde_json::Value {
    match v {
        serde_json::Value::Object(m) => {
            serde_json::Value::Object(m.into_iter().map(|(k, v)| (k.to_lowercase(), lower_deep(v))).collect())
        }
        o => o,
    }
}

/// `unmarshal_ext` for nested structs (all keys matched case-insensitively).
fn unmarshal_ext_deep<T: serde::de::DeserializeOwned + Default>(ext: Option<&Ext>) -> Result<T, BidderError> {
    let Some(ext) = ext else { return unmarshal_ext(None) };
    let v: serde_json::Value = serde_json::from_str(&ext.to_json()).unwrap_or(serde_json::Value::Null);
    if !v.is_object() {
        return unmarshal_ext(Some(ext));
    }
    serde_json::from_value(lower_deep(v)).map_err(|e| BidderError::FailedToUnmarshal(e.to_string()))
}

// ---- local helpers (shared foundation untouched) ----

/// Go `jsonutil.Unmarshal(ext, &T)`: json-iterator matches keys case-insensitively; a non-object
/// (other than null) reports `expect { or n, but found X`; absent ext is empty input.
fn unmarshal_ext<T: serde::de::DeserializeOwned + Default>(ext: Option<&Ext>) -> Result<T, BidderError> {
    use sonic_rs::JsonValueTrait;
    let Some(ext) = ext else {
        return Err(BidderError::FailedToUnmarshal("expect { or n, but found \u{0}".into()));
    };
    if ext.0.is_null() {
        return Ok(T::default());
    }
    let text = ext.to_json();
    if !ext.0.is_object() {
        let c = text.chars().next().unwrap_or('\u{0}');
        return Err(BidderError::FailedToUnmarshal(format!("expect {{ or n, but found {c}")));
    }
    let map: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&text)
        .map_err(|e| BidderError::FailedToUnmarshal(e.to_string()))?;
    let lowered: serde_json::Map<String, serde_json::Value> =
        map.into_iter().map(|(k, v)| (k.to_lowercase(), v)).collect();
    serde_json::from_value(serde_json::Value::Object(lowered))
        .map_err(|e| BidderError::FailedToUnmarshal(e.to_string()))
}

#[derive(serde::Deserialize, Default)]
#[serde(default)]
struct ExtImpBidder {
    bidder: Option<Ext>,
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

/// Go `template.Parse` rejects `{{}}` and bare identifiers (undefined functions); an unknown
/// `.Field` only fails when the template runs.
fn check_template(endpoint: &str) -> Result<(), String> {
    let t = crate::macros::EndpointTemplate::parse(endpoint)?;
    match t.resolve(&crate::macros::EndpointTemplateParams::default()) {
        Err(e) if !e.contains("function \".") => Err(e),
        _ => Ok(()),
    }
}
