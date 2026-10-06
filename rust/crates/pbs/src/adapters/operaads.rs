//! Go `adapters/operaads/operaads.go`.

use serde::Deserialize;

use crate::bid_types::BidType;
use crate::bidder::{Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::macros::{EndpointTemplate, EndpointTemplateParams};
use crate::ortb::openrtb2::{Banner, BidRequest, BidResponse, Imp};
use crate::ortb::Ext;

pub struct Adapter {
    ep_template: EndpointTemplate,
}

/// Go `openrtb_ext.ImpExtOperaads`.
#[derive(Deserialize, Default)]
#[serde(default)]
struct ImpExtOperaads {
    #[serde(rename = "placementid")]
    placement_id: String,
    #[serde(rename = "endpointid")]
    endpoint_id: String,
    #[serde(rename = "publisherid")]
    publisher_id: String,
}

enum Format {
    Native,
    Video,
    Banner,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Result<Self, BidderError> {
        let endpoint = endpoint.into();
        check_template(&endpoint).map_err(BidderError::other)?;
        Ok(Self { ep_template: EndpointTemplate::parse(&endpoint).map_err(BidderError::other)? })
    }
}

fn flat_imp(
    request: &BidRequest,
    imp: &Imp,
    headers: &Header,
    endpoint: &str,
    format: &Format,
) -> Result<RequestData, BidderError> {
    let mut imp = imp.clone();
    match format {
        Format::Video => {
            imp.native = None;
            imp.banner = None;
            imp.id = build_opera_imp_id(&imp.id, BidType::Video);
        }
        Format::Banner => {
            imp.video = None;
            imp.native = None;
            imp.id = build_opera_imp_id(&imp.id, BidType::Banner);
        }
        Format::Native => {
            imp.video = None;
            imp.banner = None;
            imp.id = build_opera_imp_id(&imp.id, BidType::Native);
        }
    }
    convert_impression(&mut imp).map_err(|e| BidderError::bad_input(e.to_string()))?;
    let mut request_copy = request.clone();
    request_copy.imp = vec![imp];
    let body = crate::go_json::to_vec(&request_copy).map_err(|e| BidderError::bad_input(e.to_string()))?;
    Ok(RequestData {
        method: "POST".into(),
        uri: endpoint.to_string(),
        body,
        headers: headers.clone(),
        imp_ids: request_copy.imp.iter().map(|i| i.id.clone()).collect(),
    })
}

fn convert_impression(imp: &mut Imp) -> Result<(), BidderError> {
    if let Some(banner) = imp.banner.as_mut() {
        convert_banner(banner)?;
    }
    if let Some(native) = imp.native.as_mut() {
        if !native.request.is_empty() {
            let v: serde_json::Map<String, serde_json::Value> = jsonutil::unmarshal_any(native.request.as_bytes())
                .map_err(|_| BidderError::other("json parse error"))
                .or_else(|_| {
                    jsonutil::unmarshal::<serde_json::Map<String, serde_json::Value>>(native.request.as_bytes())
                })?;
            if !v.contains_key("native") {
                let mut wrapper = serde_json::Map::new();
                wrapper.insert("native".into(), serde_json::Value::Object(v));
                let body = crate::go_json::to_vec(&wrapper).map_err(|e| BidderError::other(e.to_string()))?;
                native.request = String::from_utf8_lossy(&body).into_owned();
            }
        }
    }
    Ok(())
}

fn convert_banner(banner: &mut Banner) -> Result<(), BidderError> {
    let missing = banner.w.is_none() || banner.h.is_none() || banner.w == Some(0) || banner.h == Some(0);
    if missing {
        if let Some(f) = banner.format.first() {
            banner.w = Some(f.w);
            banner.h = Some(f.h);
        } else {
            return Err(BidderError::other("Size information missing for banner"));
        }
    }
    Ok(())
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut request_data = Vec::with_capacity(request.imp.len());
        let mut errs = Vec::new();
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");

        match &request.device {
            Some(d) if !d.os.is_empty() => {}
            _ => {
                return (
                    vec![],
                    vec![BidderError::bad_input("Impression is missing device OS information")],
                )
            }
        }

        for imp in &request.imp {
            let bidder_ext: ExtImpBidder = match unmarshal_ext(imp.ext.as_ref()) {
                Ok(v) => v,
                Err(e) => {
                    errs.push(BidderError::bad_input(e.to_string()));
                    continue;
                }
            };
            let opera: ImpExtOperaads = match unmarshal_ext(bidder_ext.bidder.as_ref()) {
                Ok(v) => v,
                Err(e) => {
                    errs.push(BidderError::bad_input(e.to_string()));
                    continue;
                }
            };
            let params = EndpointTemplateParams {
                publisher_id: opera.publisher_id.clone(),
                account_id: opera.endpoint_id.clone(),
                ..Default::default()
            };
            let endpoint = match self.ep_template.resolve(&params) {
                Ok(e) => e,
                Err(e) => {
                    errs.push(BidderError::bad_input(e));
                    continue;
                }
            };
            let mut imp = imp.clone();
            imp.tagid = opera.placement_id.clone();
            let mut formats = Vec::with_capacity(1);
            if imp.native.is_some() {
                formats.push(Format::Native);
            }
            if imp.video.is_some() {
                formats.push(Format::Video);
            }
            if imp.banner.is_some() {
                formats.push(Format::Banner);
            }
            for format in &formats {
                match flat_imp(request, &imp, &headers, &endpoint, format) {
                    Ok(r) => request_data.push(r),
                    Err(e) => errs.push(e),
                }
            }
        }
        (request_data, errs)
    }

    fn make_bids(
        &self,
        _internal_request: &BidRequest,
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
        let parsed: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![BidderError::bad_server_response(e.to_string())]),
        };
        let mut bid_response = BidderResponse::with_bids_capacity(1);
        for sb in parsed.seatbid {
            for mut bid in sb.bid {
                if bid.price != 0.0 {
                    let (id, t) = parse_origin_imp_id(&bid.impid);
                    bid.impid = id;
                    // Go keeps an unparsable type as the empty BidType; Banner is the closest here.
                    bid_response.bids.push(TypedBid::new(bid, t.unwrap_or(BidType::Other)));
                }
            }
        }
        (Some(bid_response), vec![])
    }
}

fn build_opera_imp_id(origin_id: &str, bid_type: BidType) -> String {
    [origin_id, "opa", bid_type.as_str()].join(":")
}

fn parse_origin_imp_id(imp_id: &str) -> (String, Option<BidType>) {
    let items: Vec<&str> = imp_id.split(':').collect();
    if items.len() < 2 {
        return (imp_id.to_string(), None);
    }
    (items[..items.len() - 2].join(":"), BidType::parse(items[items.len() - 1]).ok())
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
