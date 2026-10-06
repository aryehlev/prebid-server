//! Go `adapters/gumgum/gumgum.go`.

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

use crate::ortb::openrtb2::{Format, Publisher};

/// Go `openrtb_ext.ExtImpGumGum`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ExtImpGumGum {
    zone: String,
    #[serde(rename = "pubId")]
    pub_id: f64,
    irisid: String,
    slot: f64,
    product: String,
}

#[derive(Serialize)]
struct ExtImpGumGumVideo<'a> {
    #[serde(skip_serializing_if = "str::is_empty")]
    irisid: &'a str,
}

#[derive(Serialize)]
struct ExtImpGumGumBanner {
    #[serde(skip_serializing_if = "is_zero")]
    si: f64,
    #[serde(skip_serializing_if = "is_zero")]
    maxw: f64,
    #[serde(skip_serializing_if = "is_zero")]
    maxh: f64,
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

/// Go `openrtb_ext.ExtImpAdUnitCode` (only `prebid.adunitcode`).
#[derive(Debug, Default, Deserialize)]
struct ExtImpAdUnitCode {
    #[serde(default)]
    prebid: Option<AdUnitPrebid>,
}

#[derive(Debug, Default, Deserialize)]
struct AdUnitPrebid {
    #[serde(default)]
    adunitcode: String,
}

pub struct Adapter {
    uri: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self { uri: endpoint.into() }
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


/// Go `strconv.FormatFloat(v, 'f', -1, 64)`.
fn format_float(v: f64) -> String {
    format!("{v}")
}

fn get_bigger_format(format_list: &[Format], slot: f64) -> ExtImpGumGumBanner {
    let (mut maxw, mut maxh, mut greatest) = (0i64, 0i64, 0i64);
    for size in format_list {
        let bigger = if size.w > size.h { size.w } else { size.h };
        if bigger > greatest || (bigger == greatest && size.w >= maxw && size.h >= maxh) {
            greatest = bigger;
            maxh = size.h;
            maxw = size.w;
        }
    }
    ExtImpGumGumBanner { si: slot, maxw: maxw as f64, maxh: maxh as f64 }
}

fn get_media_type_for_imp_id(imp_id: &str, imps: &[Imp]) -> BidType {
    for imp in imps {
        if imp.id == imp_id && imp.banner.is_some() {
            return BidType::Banner;
        }
    }
    BidType::Video
}

fn preprocess(imp: &mut Imp) -> Result<ExtImpGumGum, BidderError> {
    let bidder_ext: ExtImpBidder = decode_ext(imp.ext.as_ref()).map_err(|e| BidderError::bad_input(e.to_string()))?;
    let gumgum_ext: ExtImpGumGum =
        decode_ext(bidder_ext.bidder.as_ref()).map_err(|e| BidderError::bad_input(e.to_string()))?;

    // Go uses the std `json.Unmarshal` here, ignoring errors.
    if let Ok(ext) = decode_ext::<ExtImpAdUnitCode>(imp.ext.as_ref()) {
        if let Some(p) = ext.prebid {
            if !p.adunitcode.is_empty() {
                imp.tagid = p.adunitcode;
            }
        }
    }

    if let Some(banner) = &imp.banner {
        if banner.w.is_none() && banner.h.is_none() && !banner.format.is_empty() {
            let mut banner_copy = banner.clone();
            let format = &banner_copy.format[0];
            banner_copy.w = Some(format.w);
            banner_copy.h = Some(format.h);
            if gumgum_ext.slot != 0.0 {
                let banner_ext = get_bigger_format(&banner_copy.format, gumgum_ext.slot);
                banner_copy.ext = Some(ext_from(&banner_ext)?);
            }
            imp.banner = Some(banner_copy);
        }
    }
    if let Some(video) = &imp.video {
        if !gumgum_ext.irisid.is_empty() {
            let mut video_copy = video.clone();
            video_copy.ext = Some(ext_from(&ExtImpGumGumVideo { irisid: &gumgum_ext.irisid })?);
            imp.video = Some(video_copy);
        }
    }
    if !gumgum_ext.product.is_empty() {
        let mut m = std::collections::BTreeMap::new();
        m.insert("product", gumgum_ext.product.as_str());
        imp.ext = Some(ext_from(&m)?);
    }
    Ok(gumgum_ext)
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut valid_imps = vec![];
        let mut site_copy = request.site.clone().unwrap_or_default();
        let mut errs = Vec::with_capacity(request.imp.len());

        for imp in &request.imp {
            let mut imp = imp.clone();
            match preprocess(&mut imp) {
                Err(e) => errs.push(e),
                Ok(ext) => {
                    if !ext.zone.is_empty() {
                        site_copy.id = ext.zone.clone();
                    }
                    if ext.pub_id != 0.0 {
                        match site_copy.publisher.as_mut() {
                            Some(p) => p.id = format_float(ext.pub_id),
                            None => {
                                site_copy.publisher = Some(Publisher { id: format_float(ext.pub_id), ..Default::default() })
                            }
                        }
                    }
                    valid_imps.push(imp);
                }
            }
        }
        if valid_imps.is_empty() {
            return (vec![], errs);
        }
        let mut req = request.clone();
        req.imp = valid_imps;
        if request.site.is_some() {
            req.site = Some(site_copy);
        }
        let body = match marshal(&req) {
            Ok(b) => b,
            Err(e) => {
                errs.push(e);
                return (vec![], errs);
            }
        };
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        (
            vec![RequestData {
                method: "POST".into(),
                uri: self.uri.clone(),
                body,
                headers,
                imp_ids: imp_ids(&req.imp),
            }],
            errs,
        )
    }

    fn make_bids(
        &self,
        request: &BidRequest,
        _request_data: &RequestData,
        response_data: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        let code = response_data.status_code;
        if code == 204 {
            return (None, vec![]);
        }
        if code == 400 {
            return (None, vec![BidderError::bad_input(format!("Bad user input: HTTP status {code}"))]);
        }
        if code != 200 {
            return (None, vec![BidderError::bad_server_response(format!("Bad server response: HTTP status {code}"))]);
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            // Go formats the error with `%d` (`&{%!d(string=...)}`).
            Err(e) => {
                return (
                    None,
                    vec![BidderError::bad_server_response(format!(
                        "Bad server response: &{{%!d(string={})}}. ",
                        e.message()
                    ))],
                )
            }
        };
        let mut out = BidderResponse::with_bids_capacity(5);
        for sb in bid_resp.seatbid {
            for mut bid in sb.bid {
                let media_type = get_media_type_for_imp_id(&bid.impid, &request.imp);
                if media_type == BidType::Video {
                    let price = format_float(bid.price);
                    bid.adm = bid.adm.replace("${AUCTION_PRICE}", &price);
                }
                out.bids.push(TypedBid::new(bid, media_type));
            }
        }
        if !bid_resp.cur.is_empty() {
            out.currency = bid_resp.cur.clone();
        }
        (Some(out), vec![])
    }
}
