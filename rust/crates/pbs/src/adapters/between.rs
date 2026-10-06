//! Go `adapters/between/between.go`.

use serde::Deserialize;

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

use crate::macros::{EndpointTemplate, EndpointTemplateParams};

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ExtImpBetween {
    host: String,
    publisher_id: String,
}

pub struct Adapter {
    endpoint_template: EndpointTemplate,
}

impl Adapter {
    /// Go `Builder`: fails on an endpoint that is not a valid template.
    pub fn new(endpoint: impl AsRef<str>) -> Result<Self, BidderError> {
        Ok(Self { endpoint_template: build_template(endpoint.as_ref())? })
    }

    fn build_endpoint_url(&self, e: &ExtImpBetween) -> Result<String, BidderError> {
        let missing = |p: &str| BidderError::bad_input(format!("required BetweenSSP parameter \"{p}\" is missing"));
        if e.host.is_empty() {
            return Err(missing("host"));
        }
        if e.publisher_id.is_empty() {
            return Err(missing("publisher_id"));
        }
        self.endpoint_template
            .resolve(&EndpointTemplateParams {
                host: e.host.clone(),
                publisher_id: e.publisher_id.clone(),
                ..Default::default()
            })
            .map_err(BidderError::other)
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


fn unpack_imp_ext(imp: &Imp) -> Result<ExtImpBetween, BidderError> {
    let bidder_ext: ExtImpBidder = decode_ext(imp.ext.as_ref())
        .map_err(|_| BidderError::bad_input(format!("ignoring imp id={}, invalid BidderExt", imp.id)))?;
    decode_ext(bidder_ext.bidder.as_ref())
        .map_err(|_| BidderError::bad_input(format!("ignoring imp id={}, invalid ImpExt", imp.id)))
}

fn build_imp_banner(imp: &mut Imp) -> Result<(), BidderError> {
    let Some(banner) = &imp.banner else {
        return Err(BidderError::bad_input("Request needs to include a Banner object"));
    };
    let mut banner = banner.clone();
    if banner.w.is_none() && banner.h.is_none() {
        if banner.format.is_empty() {
            return Err(BidderError::bad_input("Need at least one size to build request"));
        }
        let format = banner.format.remove(0);
        banner.w = Some(format.w);
        banner.h = Some(format.h);
        imp.banner = Some(banner);
    }
    Ok(())
}

fn add_header_if_non_empty(headers: &mut Header, name: &str, value: &str) {
    if !value.is_empty() {
        headers.add(name, value);
    }
}

/// Go `preprocess`: rewrites `request.imp` in place and returns the last imp ext decoded.
fn preprocess(request: &mut BidRequest) -> (Option<ExtImpBetween>, Vec<BidderError>) {
    let mut errors = Vec::with_capacity(request.imp.len());
    let mut res_imps = Vec::with_capacity(request.imp.len());
    let mut secure = 0i8;
    if let Some(site) = &request.site {
        if !site.page.is_empty() {
            if let Ok(u) = url::Url::parse(&site.page) {
                if u.scheme() == "https" {
                    secure = 1;
                }
            }
        }
    }
    let mut between_ext = None;
    for imp in &request.imp {
        let mut imp = imp.clone();
        // Go assigns `betweenExt` even on error (nil), so the last imp decides.
        match unpack_imp_ext(&imp) {
            Ok(e) => between_ext = Some(e),
            Err(e) => {
                between_ext = None;
                errors.push(e);
                continue;
            }
        }
        imp.secure = Some(secure);
        if let Err(e) = build_imp_banner(&mut imp) {
            errors.push(e);
            continue;
        }
        res_imps.push(imp);
    }
    request.imp = res_imps;
    (between_ext, errors)
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::bad_input("No valid Imps in Bid Request")]);
        }
        let mut request = request.clone();
        let (ext, errors) = preprocess(&mut request);
        if !errors.is_empty() {
            return (vec![], errors);
        }
        // Go dereferences the nil ext only if every imp failed, and then `errors` is non-empty.
        let ext = ext.unwrap_or_default();
        let endpoint = match self.build_endpoint_url(&ext) {
            Ok(e) => e,
            Err(e) => {
                return (vec![], vec![BidderError::bad_input(format!("Failed to build endpoint URL: {e}"))])
            }
        };
        let data = match marshal(&request) {
            Ok(d) => d,
            Err(_) => return (vec![], vec![BidderError::bad_input("Error in packaging request to JSON")]),
        };
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        if let Some(device) = &request.device {
            add_header_if_non_empty(&mut headers, "User-Agent", &device.ua);
            add_header_if_non_empty(&mut headers, "X-Forwarded-For", &device.ip);
            add_header_if_non_empty(&mut headers, "Accept-Language", &device.language);
            if let Some(dnt) = device.dnt {
                add_header_if_non_empty(&mut headers, "DNT", &dnt.to_string());
            }
        }
        if let Some(site) = &request.site {
            add_header_if_non_empty(&mut headers, "Referer", &site.page);
        }
        (
            vec![RequestData { method: "POST".into(), uri: endpoint, body: data, headers, imp_ids: imp_ids(&request.imp) }],
            errors,
        )
    }

    fn make_bids(
        &self,
        _request: &BidRequest,
        _request_data: &RequestData,
        response_data: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if response_data.status_code == 204 {
            return (None, vec![]);
        }
        if response_data.status_code != 200 {
            return (
                None,
                vec![BidderError::bad_server_response(format!(
                    "Invalid Status Returned: {}. Run with request.debug = 1 for more info",
                    response_data.status_code
                ))],
            );
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(e) => {
                return (
                    None,
                    vec![BidderError::bad_server_response(format!("Unable to unpackage bid response. Error {e}"))],
                )
            }
        };
        let mut out = BidderResponse::with_bids_capacity(1);
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                out.bids.push(TypedBid::new(bid, BidType::Banner));
            }
        }
        (Some(out), vec![])
    }
}
