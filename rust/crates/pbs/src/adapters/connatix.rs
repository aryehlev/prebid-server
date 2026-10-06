//! Go `adapters/connatix/connatix.go`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::bid_types::BidType;
use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder, BidderResponse,
    ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{BidRequest, BidResponse, Imp};
use crate::ortb::Ext;

use crate::ext_helpers::ext_get;

const MAX_IMPS_PER_REQ: usize = 1;

/// Go `openrtb_ext.ExtImpConnatix`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ExtImpConnatix {
    #[serde(rename = "placementId")]
    placement_id: String,
    #[serde(rename = "viewabilityPercentage")]
    viewability_percentage: f64,
}

#[derive(Serialize)]
struct ImpExtConnatix<'a> {
    #[serde(rename = "placementId", skip_serializing_if = "str::is_empty")]
    placement_id: &'a str,
    #[serde(rename = "viewabilityPercentage", skip_serializing_if = "is_zero")]
    viewability_percentage: f64,
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

#[derive(Debug, Default, Deserialize)]
struct BidExt {
    #[serde(default)]
    connatix: Option<BidCnxExt>,
}

#[derive(Debug, Default, Deserialize)]
struct BidCnxExt {
    #[serde(rename = "mediaType", default)]
    media_type: String,
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


/// Go `jsonparser.GetString(req.App.Ext, "prebid", key)`: only a string value counts.
fn app_prebid_string(request: &BidRequest, key: &str) -> Option<String> {
    use sonic_rs::JsonValueTrait;
    let app = request.app.as_ref()?;
    ext_get(app.ext.as_ref(), &["prebid", key])?.as_str().map(str::to_string)
}

fn build_display_manager_ver(request: &BidRequest) -> String {
    if request.app.is_none() {
        return String::new();
    }
    let Some(source) = app_prebid_string(request, "source") else { return String::new() };
    let Some(version) = app_prebid_string(request, "version") else { return String::new() };
    format!("{source}-{version}")
}

fn validate_and_build_imp_ext(imp: &Imp) -> Result<ExtImpConnatix, BidderError> {
    let bidder_ext: ExtImpBidder = decode_ext(imp.ext.as_ref())?;
    check_string_fields(bidder_ext.bidder.as_ref(), "openrtb_ext.ExtImpConnatix", &[("placementId", "PlacementId")])?;
    if bidder_ext.bidder.is_none() {
        return Ok(ExtImpConnatix::default());
    }
    decode_ext(bidder_ext.bidder.as_ref())
}

fn build_request_imp(
    imp: &mut Imp,
    ext: &ExtImpConnatix,
    display_manager_ver: &str,
    req_info: &ExtraRequestInfo,
) -> Result<(), BidderError> {
    if let Some(banner) = &imp.banner {
        let mut banner_copy = banner.clone();
        if banner_copy.w.is_none() && banner_copy.h.is_none() && !banner_copy.format.is_empty() {
            let first = &banner_copy.format[0];
            banner_copy.w = Some(first.w);
            banner_copy.h = Some(first.h);
        }
        imp.banner = Some(banner_copy);
    }

    // Populate imp.displaymanagerver if the SDK failed to do it.
    if imp.displaymanagerver.is_empty() && !display_manager_ver.is_empty() {
        imp.displaymanagerver = display_manager_ver.to_string();
    }

    // Check if imp comes with bid floor amount defined in a foreign currency
    if imp.bidfloor > 0.0 && !imp.bidfloorcur.is_empty() && !imp.bidfloorcur.eq_ignore_ascii_case("USD") {
        let converted = req_info.convert_currency(imp.bidfloor, &imp.bidfloorcur, "USD")?;
        imp.bidfloorcur = "USD".to_string();
        imp.bidfloor = converted;
    }

    // Go: unmarshal into map[string]interface{}; any failure starts from an empty map. Marshalling
    // a Go map sorts the keys (serde_json's Map is ordered by key too).
    let mut incoming: serde_json::Map<String, Value> = match imp.ext.as_ref() {
        Some(e) => serde_json::from_str(&e.to_json()).unwrap_or_default(),
        None => serde_json::Map::new(),
    };
    incoming.remove("bidder");
    let cnx = serde_json::to_value(ImpExtConnatix {
        placement_id: &ext.placement_id,
        viewability_percentage: ext.viewability_percentage,
    })
    .map_err(|e| BidderError::other(e.to_string()))?;
    incoming.insert("connatix".to_string(), cnx);
    imp.ext = Some(ext_from(&incoming)?);
    Ok(())
}

/// Go `url.Values.Encode` for the single `dc` key, set on the parsed endpoint.
fn endpoint_with_dc(uri: &str, dc: Option<&str>) -> String {
    let (rest, fragment) = match uri.split_once('#') {
        Some((a, f)) => (a, Some(f)),
        None => (uri, None),
    };
    let base = rest.split_once('?').map_or(rest, |(b, _)| b);
    let mut out = base.to_string();
    if let Some(dc) = dc {
        out.push_str("?dc=");
        out.push_str(dc);
    }
    if let Some(f) = fragment {
        out.push('#');
        out.push_str(f);
    }
    out
}

fn split_requests(imps: &[Imp], request: &BidRequest, uri: &str) -> (Vec<RequestData>, Vec<BidderError>) {
    let mut res = Vec::with_capacity((imps.len() + MAX_IMPS_PER_REQ - 1) / MAX_IMPS_PER_REQ);
    let mut headers = Header::new();
    headers.add("Content-Type", "application/json");
    headers.add("Accept", "application/json");
    if let Some(device) = &request.device {
        if !device.ua.is_empty() {
            headers.add("User-Agent", device.ua.clone());
        }
        if !device.ipv6.is_empty() {
            headers.add("X-Forwarded-For", device.ipv6.clone());
        }
        if !device.ip.is_empty() {
            headers.add("X-Forwarded-For", device.ip.clone());
        }
    }

    let mut req = request.clone();
    for chunk in imps.chunks(MAX_IMPS_PER_REQ) {
        req.imp = chunk.to_vec();
        let body = match marshal(&req) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![e]),
        };
        let mut dc = None;
        let mut touch_query = false;
        if let Some(user) = &req.user {
            let user_id = user.buyeruid.trim();
            if !user_id.is_empty() {
                touch_query = true;
                if user_id.starts_with("1-") {
                    dc = Some("us-east-2");
                } else if user_id.starts_with("2-") {
                    dc = Some("us-west-2");
                } else if user_id.starts_with("3-") {
                    dc = Some("eu-west-1");
                }
            }
        }
        let uri = if touch_query { endpoint_with_dc(uri, dc) } else { uri.to_string() };
        res.push(RequestData {
            method: "POST".into(),
            uri,
            body,
            headers: headers.clone(),
            imp_ids: imp_ids(&req.imp),
        });
    }
    (res, vec![])
}

fn get_bid_type(ext: &BidExt) -> BidType {
    if ext.connatix.as_ref().is_some_and(|c| c.media_type == "video") {
        return BidType::Video;
    }
    BidType::Banner
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        match &request.device {
            Some(d) if !(d.ip.is_empty() && d.ipv6.is_empty()) => {}
            _ => return (vec![], vec![BidderError::bad_input("Device IP is required")]),
        }

        // connatix adapter expects imp.displaymanagerver to be populated in openrtb2 request
        // but some SDKs will put it in imp.ext.prebid instead
        let display_manager_ver = build_display_manager_ver(request);

        let mut errs = vec![];
        let mut valid_imps = vec![];
        for imp in &request.imp {
            let mut imp = imp.clone();
            let ext = match validate_and_build_imp_ext(&imp) {
                Ok(e) => e,
                Err(e) => {
                    errs.push(e);
                    continue;
                }
            };
            if let Err(e) = build_request_imp(&mut imp, &ext, &display_manager_ver, req_info) {
                errs.push(e);
                continue;
            }
            valid_imps.push(imp);
        }

        let (requests, errors) = split_requests(&valid_imps, request, &self.endpoint);
        errs.extend(errors);
        (requests, errs)
    }

    fn make_bids(
        &self,
        _request: &BidRequest,
        _request_data: &RequestData,
        response_data: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if is_response_status_code_no_content(response_data) {
            return (None, vec![]);
        }
        if let Some(err) = check_response_status_code_for_errors(response_data) {
            return (None, vec![err]);
        }
        let response: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut out = BidderResponse::with_bids_capacity(1);
        for sb in response.seatbid {
            for bid in sb.bid {
                let bid_type = match decode_ext::<BidExt>(bid.ext.as_ref()) {
                    Err(_) => BidType::Banner,
                    Ok(ext) => get_bid_type(&ext),
                };
                out.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        out.currency = "USD".to_string();
        (Some(out), vec![])
    }
}
