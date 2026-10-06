//! Go `adapters/33across/33across.go` (package `ttx`).

#![allow(unused_imports, dead_code)]
use crate::bid_types::BidType;
use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder, BidderResponse,
    ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::macros::{EndpointTemplate, EndpointTemplateParams};
use crate::ortb::openrtb2::{Bid, BidRequest, BidResponse, Imp, MarkupType};
use crate::ortb::Ext;
use serde::{Deserialize, Serialize};

use crate::ortb::adcom1::StartDelay;
use crate::ortb::openrtb2::Video;

#[derive(Deserialize, Default)]
struct ExtImp33across {
    #[serde(rename = "zoneId", default)]
    zone_id: String,
    #[serde(rename = "siteId", default)]
    site_id: String,
    #[serde(rename = "productId", default)]
    product_id: String,
}

#[derive(Serialize, Deserialize, Default)]
struct ImpTtxExt {
    #[serde(default)]
    prod: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    zoneid: String,
}

#[derive(Serialize, Deserialize, Default)]
struct ImpExt {
    #[serde(default)]
    ttx: ImpTtxExt,
}

#[derive(Serialize, Deserialize, Default)]
struct ReqExt {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ttx: Option<ReqTtxExt>,
}

#[derive(Serialize, Deserialize, Default)]
struct ReqTtxExt {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    caller: Option<Vec<TtxCaller>>,
}

#[derive(Serialize, Deserialize, Clone)]
struct TtxCaller {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    version: String,
}

#[derive(Deserialize, Default)]
struct BidTtxExt {
    #[serde(rename = "mediaType", default)]
    media_type: String,
}

#[derive(Deserialize, Default)]
struct BidExt {
    #[serde(default)]
    ttx: BidTtxExt,
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

/// Go `makeReqExt`.
fn make_req_ext(request: &BidRequest) -> Result<Ext, BidderError> {
    let mut req_ext: ReqExt = match &request.ext {
        Some(e) if !e.to_json().is_empty() => unmarshal_raw(e.to_json().as_bytes())?,
        _ => ReqExt::default(),
    };
    let ttx = req_ext.ttx.get_or_insert_with(ReqTtxExt::default);
    let callers = ttx.caller.get_or_insert_with(Vec::new);
    callers.push(TtxCaller { name: "Prebid-Server".into(), version: "n/a".into() });
    ext_from(&req_ext)
}

fn validate_video_params(video: &Video, prod: &str) -> Result<Video, BidderError> {
    let mut video = video.clone();
    // Go compares the slices to nil; empty is treated the same here.
    if video.w.unwrap_or_default() == 0 || video.h.unwrap_or_default() == 0 || video.protocols.is_empty() || video.mimes.is_none() || video.playbackmethod.is_empty() {
        return Err(BidderError::bad_input(
            "One or more invalid or missing video field(s) w, h, protocols, mimes, playbackmethod",
        ));
    }
    if prod == "instream" && video.startdelay.is_none() {
        video.startdelay = Some(StartDelay(0));
    }
    Ok(video)
}

fn make_imps(imp: &Imp) -> Result<Imp, BidderError> {
    if imp.banner.is_none() && imp.video.is_none() {
        return Err(BidderError::bad_input(format!(
            "Imp ID {} must have at least one of [Banner, Video] defined",
            imp.id
        )));
    }
    let bidder = imp_bidder_raw(&imp.ext).map_err(|e| BidderError::bad_input(e.to_string()))?;
    let ttx_ext: ExtImp33across = unmarshal_raw(&bidder).map_err(|e| BidderError::bad_input(e.to_string()))?;

    let mut imp_ext = ImpExt::default();
    imp_ext.ttx.prod = ttx_ext.product_id;
    imp_ext.ttx.zoneid = ttx_ext.site_id;
    if !ttx_ext.zone_id.is_empty() {
        imp_ext.ttx.zoneid = ttx_ext.zone_id;
    }
    let mut imp = imp.clone();
    imp.ext = Some(ext_from(&imp_ext).map_err(|e| BidderError::bad_input(e.to_string()))?);

    if let Some(video) = &imp.video {
        // Go assigns `imp.Video = videoCopy` even on error, but the imp is dropped then.
        imp.video = Some(validate_video_params(video, &imp_ext.ttx.prod)?);
    }
    Ok(imp)
}

impl Bidder for Adapter {
    fn make_requests(&self, request: &BidRequest, _req_info: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = vec![];
        let mut adapter_requests = vec![];
        // Go ranges over a map (random order); first-seen order here.
        let mut grouped: Vec<(String, Vec<Imp>)> = vec![];

        // Construct request extension common to all imps. Not blocking adapter requests on
        // errors since request extension is optional. (On error Go sets `request.Ext = nil`.)
        let mut req = request.clone();
        match make_req_ext(request) {
            Ok(e) => req.ext = Some(e),
            Err(e) => {
                errs.push(e);
                req.ext = None;
            }
        }

        // We only support SRA for requests containing same prod and zoneID, therefore group all
        // imps accordingly and create a http request for each such group.
        for imp in &request.imp {
            match make_imps(imp) {
                Ok(imp_copy) => {
                    // Skip over imps whose extensions cannot be read.
                    match unmarshal_raw::<ImpExt>(&ext_bytes(&imp_copy.ext)) {
                        Ok(imp_ext) => {
                            let key = format!("{}{}", imp_ext.ttx.prod, imp_ext.ttx.zoneid);
                            match grouped.iter_mut().find(|(k, _)| *k == key) {
                                Some((_, v)) => v.push(imp_copy),
                                None => grouped.push((key, vec![imp_copy])),
                            }
                        }
                        Err(e) => errs.push(e),
                    }
                }
                Err(e) => errs.push(e),
            }
        }

        for (_, imp_list) in grouped {
            let mut r = req.clone();
            r.imp = imp_list;
            match marshal(&r) {
                Ok(body) => {
                    let mut headers = Header::new();
                    headers.add("Content-Type", "application/json;charset=utf-8");
                    adapter_requests.push(RequestData {
                        method: "POST".into(),
                        uri: self.endpoint.clone(),
                        body,
                        headers,
                        imp_ids: imp_ids(&r.imp),
                    });
                }
                Err(e) => errs.push(e),
            }
        }
        (adapter_requests, errs)
    }

    fn make_bids(
        &self,
        _request: &BidRequest,
        _request_data: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        match response.status_code {
            204 => return (None, vec![]),
            400 => {
                return (
                    None,
                    vec![BidderError::bad_input(format!(
                        "Unexpected status code: {}. Run with request.debug = 1 for more info",
                        response.status_code
                    ))],
                )
            }
            200 => {}
            c => {
                return (
                    None,
                    vec![BidderError::bad_server_response(format!(
                        "Unexpected status code: {c}. Run with request.debug = 1 for more info"
                    ))],
                )
            }
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut bid_response = BidderResponse::with_bids_capacity(1);
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = match unmarshal_raw::<BidExt>(&ext_bytes(&bid.ext)) {
                    Err(_) => BidType::Banner,
                    Ok(e) if e.ttx.media_type == "video" => BidType::Video,
                    Ok(_) => BidType::Banner,
                };
                bid_response.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        (Some(bid_response), vec![])
    }
}

// ---- local helpers (Go `adapters.ExtImpBidder` + `jsonutil.Unmarshal` on raw ext bytes) ----

#[derive(Debug, Default, serde::Deserialize)]
struct ExtImpBidder {
    #[serde(default)]
    bidder: Option<Ext>,
}

/// Go `jsonutil.Unmarshal(raw, &v)` where `raw` may be empty (nil `json.RawMessage`): json-iterator
/// reports the NUL it reads past the end of the input. `null` leaves `v` at its zero value.
fn unmarshal_raw<T: serde::de::DeserializeOwned + Default>(raw: &[u8]) -> Result<T, BidderError> {
    if raw.is_empty() {
        return Err(BidderError::FailedToUnmarshal("expect { or n, but found \u{0}".to_string()));
    }
    if raw.iter().find(|b| !b" \t\r\n".contains(b)) == Some(&b'n') && raw.trim_ascii() == b"null" {
        return Ok(T::default());
    }
    jsonutil::unmarshal(raw)
}

fn ext_bytes(ext: &Option<Ext>) -> Vec<u8> {
    ext.as_ref().map(|e| e.to_json().into_bytes()).unwrap_or_default()
}

/// Go `jsonutil.Unmarshal(imp.Ext, &adapters.ExtImpBidder)`, returning `bidderExt.Bidder` bytes
/// (empty when absent).
fn imp_bidder_raw(ext: &Option<Ext>) -> Result<Vec<u8>, BidderError> {
    let parsed: ExtImpBidder = unmarshal_raw(&ext_bytes(ext))?;
    Ok(parsed.bidder.map(|b| b.to_json().into_bytes()).unwrap_or_default())
}

/// Go `json.Marshal(v)` into a `json.RawMessage` stand-in.
fn ext_from<T: serde::Serialize>(v: &T) -> Result<Ext, BidderError> {
    let bytes = crate::go_json::to_vec(v).map_err(|e| BidderError::other(e.to_string()))?;
    Ext::from_slice(&bytes).map_err(|e| BidderError::other(e.to_string()))
}

/// Go `openrtb_ext.GetImpIDs`.
fn imp_ids(imps: &[Imp]) -> Vec<String> {
    imps.iter().map(|i| i.id.clone()).collect()
}

/// Go `reqCopy := *request` then replacing `Imp`: a copy of the request without its imps, plus the
/// imps (cloned once).
fn split_request(request: &BidRequest) -> (BidRequest, Vec<Imp>) {
    let mut base = request.clone();
    let imps = std::mem::take(&mut base.imp);
    (base, imps)
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

fn marshal(request: &BidRequest) -> Result<Vec<u8>, BidderError> {
    crate::go_json::to_vec(request).map_err(|e| BidderError::other(e.to_string()))
}

/// jsoniter's struct-path message for a JSON string field holding a non-string value, e.g.
/// `cannot unmarshal openrtb_ext.ExtImpAJA.AdSpotID: expects " or n, but found 1`. Checked before
/// the serde decode, which words it differently. `fields` is `(json key, Go field name)`.
fn check_string_fields(raw: &[u8], go_struct: &str, fields: &[(&str, &str)]) -> Result<(), BidderError> {
    let Ok(serde_json::Value::Object(obj)) = serde_json::from_slice::<serde_json::Value>(raw) else {
        return Ok(());
    };
    for (key, go_field) in fields {
        if let Some(v) = obj.get(*key) {
            let found = match v {
                serde_json::Value::Null | serde_json::Value::String(_) => continue,
                serde_json::Value::Array(_) => '[',
                serde_json::Value::Object(_) => '{',
                serde_json::Value::Bool(true) => 't',
                serde_json::Value::Bool(false) => 'f',
                serde_json::Value::Number(n) => n.to_string().chars().next().unwrap_or('0'),
            };
            return Err(BidderError::FailedToUnmarshal(format!(
                "cannot unmarshal {go_struct}.{go_field}: expects \" or n, but found {found}"
            )));
        }
    }
    Ok(())
}
