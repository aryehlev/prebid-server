//! Go `adapters/openx/openx.go`.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::bid_types::BidType;
use crate::bidder::{
    Bidder, BidderResponse,
    ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{Bid, BidRequest, BidResponse, Imp};
use crate::ortb::Ext;

use crate::bid_types::{ExtBidPrebidMeta, ExtBidPrebidVideo};

const HBCONFIG: &str = "hb_pbs_1.0.0";

/// Go `openrtb_ext.ExtImpOpenx`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ExtImpOpenx {
    unit: JsonNumber,
    platform: String,
    #[serde(rename = "delDomain")]
    del_domain: String,
    #[serde(rename = "customFloor")]
    custom_floor: JsonNumber,
    #[serde(rename = "customParams")]
    custom_params: Option<Map<String, Value>>,
}

#[derive(Serialize)]
struct OpenxReqExt {
    #[serde(rename = "delDomain", skip_serializing_if = "String::is_empty")]
    del_domain: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    platform: String,
    bc: String,
}

pub struct Adapter {
    bidder_name: String,
    endpoint: String,
}

impl Adapter {
    /// Go `Builder` (`bidder_name` is the registered bidder name, `openx`).
    pub fn new(endpoint: impl Into<String>, bidder_name: impl Into<String>) -> Self {
        Self { endpoint: endpoint.into(), bidder_name: bidder_name.into() }
    }

    fn make_request(&self, request: &BidRequest) -> (Option<RequestData>, Vec<BidderError>) {
        let mut errs = vec![];
        let mut valid_imps = vec![];
        let mut req_ext = OpenxReqExt { del_domain: String::new(), platform: String::new(), bc: HBCONFIG.to_string() };

        for imp in &request.imp {
            let mut imp = imp.clone();
            if let Err(e) = preprocess(&mut imp, &mut req_ext) {
                errs.push(e);
                continue;
            }
            valid_imps.push(imp);
        }

        // If all the imps were malformed, don't bother making a server call with no impressions.
        if valid_imps.is_empty() {
            return (None, errs);
        }
        let mut req = request.clone();
        req.imp = valid_imps;
        match ext_from(&req_ext) {
            Ok(e) => req.ext = Some(e),
            Err(e) => {
                errs.push(e);
                return (None, errs);
            }
        }
        let body = match marshal(&req) {
            Ok(b) => b,
            Err(e) => {
                errs.push(e);
                return (None, errs);
            }
        };
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        (
            Some(RequestData {
                method: "POST".into(),
                uri: self.endpoint.clone(),
                body,
                headers,
                imp_ids: imp_ids(&req.imp),
            }),
            errs,
        )
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


/// Mutate the imp to get it ready to send to openx.
fn preprocess(imp: &mut Imp, req_ext: &mut OpenxReqExt) -> Result<(), BidderError> {
    let bidder_ext: ExtImpBidder = decode_ext(imp.ext.as_ref()).map_err(|e| BidderError::bad_input(e.to_string()))?;
    // json.Number accepts a number or a string; other JSON types fail the decode.
    let openx_ext: ExtImpOpenx =
        decode_ext(bidder_ext.bidder.as_ref()).map_err(|e| BidderError::bad_input(e.to_string()))?;

    req_ext.del_domain = openx_ext.del_domain.clone();
    req_ext.platform = openx_ext.platform.clone();

    imp.tagid = openx_ext.unit.as_str().to_string();
    if imp.bidfloor == 0.0 {
        if let Ok(custom_floor) = openx_ext.custom_floor.float64() {
            if custom_floor > 0.0 {
                imp.bidfloor = custom_floor;
            }
        }
    }

    // outgoing imp.ext should be same as incoming imp.ext minus prebid and bidder
    let mut imp_ext: Map<String, Value> =
        decode_ext(imp.ext.as_ref()).map_err(|e| BidderError::bad_input(e.to_string()))?;
    imp_ext.remove("prebid");
    imp_ext.remove("bidder");

    if let Some(params) = &openx_ext.custom_params {
        imp_ext.insert("customParams".to_string(), Value::Object(params.clone()));
    }

    if !imp_ext.is_empty() {
        imp.ext = Some(ext_from(&imp_ext).map_err(|e| BidderError::bad_input(e.to_string()))?);
    } else {
        imp.ext = None;
    }

    if let Some(video) = &imp.video {
        let mut video_copy = video.clone();
        if imp.rwdd == 1 {
            video_copy.ext = Some(Ext::from_slice(br#"{"rewarded":1}"#).map_err(|e| BidderError::other(e.to_string()))?);
        } else {
            video_copy.ext = None;
        }
        imp.video = Some(video_copy);
    }
    Ok(())
}

fn get_bid_video(bid: &Bid) -> ExtBidPrebidVideo {
    let primary_category = bid.cat.first().cloned().unwrap_or_default();
    ExtBidPrebidVideo { primary_category, duration: bid.dur as i32 }
}

fn get_bid_type(mtype: i8, imp_id: &str, imps: &[Imp]) -> BidType {
    match mtype {
        1 => BidType::Banner,
        2 => BidType::Video,
        4 => BidType::Native,
        _ => get_media_type_for_imp(imp_id, imps),
    }
}

/// OpenX doesn't support multi-type impressions.
/// If both banner and video exist, take banner as we do not want in-banner video.
/// If both video and native exist and banner is nil, take video.
/// If both banner and native exist, take banner.
/// If all of the types (banner, video, native) exist, take banner.
fn get_media_type_for_imp(imp_id: &str, imps: &[Imp]) -> BidType {
    let mut media_type = BidType::Banner;
    for imp in imps {
        if imp.id == imp_id {
            if imp.banner.is_none() && imp.video.is_some() {
                media_type = BidType::Video;
            } else if imp.banner.is_none() && imp.native.is_some() {
                media_type = BidType::Native;
            }
            return media_type;
        }
    }
    media_type
}

/// Go `oxBidExt`: `dsp_id` and `brand_id` are `json:",string"` ints, `buyer_id` a string.
/// Any field of the wrong type fails the whole decode (no meta).
fn get_bid_meta(bid: &Bid) -> Option<ExtBidPrebidMeta> {
    let ext = bid.ext.as_ref()?;
    let v: Value = serde_json::from_str(&ext.to_json()).ok()?;
    let obj = v.as_object()?;
    let string_int = |key: &str| -> Option<i32> {
        match obj.get(key) {
            None | Some(Value::Null) => Some(0),
            Some(Value::String(s)) => s.parse::<i64>().ok().map(|n| n as i32),
            Some(_) => None,
        }
    };
    let dsp_id = string_int("dsp_id")?;
    let brand_id = string_int("brand_id")?;
    let buyer_id_text = match obj.get("buyer_id") {
        None | Some(Value::Null) => "",
        Some(Value::String(s)) => s.as_str(),
        Some(_) => return None,
    };
    let buyer_id = if buyer_id_text.is_empty() { 0 } else { buyer_id_text.parse::<i64>().map(|n| n as i32).unwrap_or(0) };
    if buyer_id <= 0 && dsp_id <= 0 && brand_id <= 0 {
        return None;
    }
    Some(ExtBidPrebidMeta { network_id: dsp_id, advertiser_id: buyer_id, brand_id, ..Default::default() })
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = vec![];
        let mut banner_and_native_imps = vec![];
        let mut video_imps = vec![];

        for imp in &request.imp {
            // OpenX doesn't allow multi-type imp. Banner takes priority over video and video takes
            // priority over native. Openx also wants to send banner and native imps in one request.
            if imp.banner.is_some() {
                banner_and_native_imps.push(imp.clone());
            } else if imp.video.is_some() {
                video_imps.push(imp.clone());
            } else if imp.native.is_some() {
                banner_and_native_imps.push(imp.clone());
            }
        }

        let mut adapter_requests = vec![];
        let mut req_copy = request.clone();

        req_copy.imp = banner_and_native_imps;
        let (adapter_req, errors) = self.make_request(&req_copy);
        if let Some(r) = adapter_req {
            adapter_requests.push(r);
        }
        errs.extend(errors);

        // OpenX only supports single imp video request
        for video_imp in video_imps {
            req_copy.imp = vec![video_imp];
            let (adapter_req, errors) = self.make_request(&req_copy);
            if let Some(r) = adapter_req {
                adapter_requests.push(r);
            }
            errs.extend(errors);
        }
        (adapter_requests, errs)
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
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut out = BidderResponse::with_bids_capacity(5);
        // override default currency
        if !bid_resp.cur.is_empty() {
            out.currency = bid_resp.cur.clone();
        }

        if let Some(ext) = &bid_resp.ext {
            if let Ok(resp_ext) = decode_ext::<Map<String, Value>>(Some(ext)) {
                // Go `openxRespExt.FledgeAuctionConfigs map[string]json.RawMessage`: a non-object
                // value fails the decode and is ignored.
                if let Some(Value::Object(configs)) = resp_ext.get("fledge_auction_configs") {
                    let list: Vec<Value> = configs
                        .iter()
                        .map(|(imp_id, config)| {
                            serde_json::json!({"impid": imp_id, "bidder": self.bidder_name, "config": config})
                        })
                        .collect();
                    if let Ok(e) = ext_from(&list) {
                        out.fledge_auction_configs = Some(e);
                    }
                }
            }
        }

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = get_bid_type(bid.mtype.0, &bid.impid, &request.imp);
                let bid_video = get_bid_video(&bid);
                let bid_meta = get_bid_meta(&bid);
                let mut tb = TypedBid::new(bid, bid_type);
                tb.bid_video = Some(bid_video);
                tb.bid_meta = bid_meta;
                out.bids.push(tb);
            }
        }
        (Some(out), vec![])
    }
}
