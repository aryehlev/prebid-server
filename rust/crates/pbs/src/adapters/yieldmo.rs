//! Go `adapters/yieldmo/yieldmo.go`.

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

#[derive(Deserialize, Default)]
struct ExtImpBidderYieldmo {
    #[serde(default)]
    bidder: Option<Ext>,
    #[serde(default)]
    data: Option<ExtData>,
}

#[derive(Deserialize, Default)]
struct ExtData {
    #[serde(default)]
    pbadslot: String,
}

#[derive(Deserialize, Default)]
struct ExtImpYieldmo {
    #[serde(rename = "placementId", default)]
    placement_id: String,
}

#[derive(Serialize)]
struct YmExt {
    placement_id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    gpid: String,
}

#[derive(Deserialize, Default)]
struct ExtBid {
    #[serde(default)]
    mediatype: String,
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

/// Mutate the request to get it ready to send to yieldmo.
fn preprocess(request: &mut BidRequest, req_info: &ExtraRequestInfo) -> Vec<BidderError> {
    let mut errs = vec![];
    for imp in &mut request.imp {
        if imp.bidfloor > 0.0 && !imp.bidfloorcur.is_empty() && imp.bidfloorcur.to_uppercase() != "USD" {
            match req_info.convert_currency(imp.bidfloor, &imp.bidfloorcur, "USD") {
                Err(_) => errs.push(BidderError::bad_input(format!(
                    "Unable to convert provided bid floor currency from {} to USD",
                    imp.bidfloorcur
                ))),
                Ok(floor) => {
                    imp.bidfloorcur = "USD".into();
                    imp.bidfloor = floor;
                }
            }
        }

        // Go keeps going after an unmarshal error with whatever was decoded (nothing).
        let bidder_ext: ExtImpBidderYieldmo = match unmarshal_raw(&ext_bytes(&imp.ext)) {
            Ok(b) => b,
            Err(e) => {
                errs.push(BidderError::bad_input(e.to_string()));
                ExtImpBidderYieldmo::default()
            }
        };
        let bidder_raw = bidder_ext.bidder.as_ref().map(|b| b.to_json().into_bytes()).unwrap_or_default();
        let ym: ExtImpYieldmo = match unmarshal_raw(&bidder_raw) {
            Ok(b) => b,
            Err(e) => {
                errs.push(BidderError::bad_input(e.to_string()));
                ExtImpYieldmo::default()
            }
        };
        let mut imp_ext = YmExt { placement_id: ym.placement_id, gpid: String::new() };
        if let Some(data) = &bidder_ext.data {
            if !data.pbadslot.is_empty() {
                imp_ext.gpid = data.pbadslot.clone();
            }
        }
        match ext_from(&imp_ext) {
            Ok(e) => imp.ext = Some(e),
            Err(e) => {
                errs.push(BidderError::bad_input(e.to_string()));
                imp.ext = None;
            }
        }
    }
    errs
}

impl Bidder for Adapter {
    fn make_requests(&self, request: &BidRequest, req_info: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut request = request.clone();
        let mut errs = preprocess(&mut request, req_info);
        let body = match marshal(&request) {
            Ok(b) => b,
            Err(e) => {
                errs.push(e);
                return (vec![], errs);
            }
        };
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        (
            vec![RequestData { method: "POST".into(), uri: self.endpoint.clone(), body, headers, imp_ids: imp_ids(&request.imp) }],
            errs,
        )
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
                // Go skips bids whose media type can't be determined.
                let Ok(t) = media_type_for_bid(&bid) else { continue };
                bid_response.bids.push(TypedBid::new(bid, t));
            }
        }
        (Some(bid_response), vec![])
    }
}

/// Retrieve the media type corresponding to the bid from the bid.ext object.
fn media_type_for_bid(bid: &Bid) -> Result<BidType, BidderError> {
    let ext: ExtBid = unmarshal_raw(&ext_bytes(&bid.ext)).map_err(|e| BidderError::bad_input(e.to_string()))?;
    match ext.mediatype.as_str() {
        "banner" => Ok(BidType::Banner),
        "video" => Ok(BidType::Video),
        "native" => Ok(BidType::Native),
        other => Err(BidderError::other(format!("invalid BidType: {other}"))),
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
