//! Go `adapters/zeroclickfraud/zeroclickfraud.go`.

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

#[derive(Deserialize, Default, Clone, PartialEq, Eq, Hash)]
struct ExtImpZeroClickFraud {
    #[serde(rename = "sourceId", default)]
    source_id: i64,
    #[serde(default)]
    host: String,
}

pub struct Adapter {
    endpoint_template: EndpointTemplate,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl AsRef<str>) -> Result<Self, BidderError> {
        let endpoint_template = parse_template(endpoint.as_ref())
            .map_err(|e| BidderError::other(format!("unable to parse endpoint url template: {e}")))?;
        Ok(Self { endpoint_template })
    }
}

fn get_bidder_params(imp: &Imp) -> Result<ExtImpZeroClickFraud, BidderError> {
    let bidder = imp_bidder_raw(&imp.ext)
        .map_err(|e| BidderError::bad_input(format!("Missing bidder ext: {e}")))?;
    let ext: ExtImpZeroClickFraud = unmarshal_raw(&bidder)
        .map_err(|e| BidderError::bad_input(format!("Cannot Resolve host or sourceId: {e}")))?;
    if ext.source_id < 1 {
        return Err(BidderError::bad_input("Invalid/Missing SourceId"));
    }
    if ext.host.is_empty() {
        return Err(BidderError::bad_input("Invalid/Missing Host"));
    }
    Ok(ext)
}

impl Bidder for Adapter {
    fn make_requests(&self, request: &BidRequest, _req_info: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = vec![];
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json");
        headers.add("Accept", "application/json");

        // splitImpressions: Go returns a nil map on the first bad imp (dropping everything). The
        // Go map iteration order is random; first-seen order is used here.
        let mut groups: Vec<(ExtImpZeroClickFraud, Vec<Imp>)> = vec![];
        let mut failed = false;
        for imp in &request.imp {
            match get_bidder_params(imp) {
                Ok(p) => match groups.iter_mut().find(|(k, _)| *k == p) {
                    Some((_, v)) => v.push(imp.clone()),
                    None => groups.push((p, vec![imp.clone()])),
                },
                Err(e) => {
                    errs.push(e);
                    failed = true;
                    break;
                }
            }
        }
        if failed {
            groups.clear();
        }

        let mut requests = vec![];
        for (ext, imps) in groups {
            let mut req = request.clone();
            req.imp = imps;
            let body = match marshal(&req) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(e);
                    continue;
                }
            };
            let params = EndpointTemplateParams { host: ext.host, source_id: ext.source_id.to_string(), ..Default::default() };
            let url = match self.endpoint_template.resolve(&params) {
                Ok(u) => u,
                Err(e) => {
                    errs.push(BidderError::other(e));
                    continue;
                }
            };
            requests.push(RequestData {
                method: "POST".into(),
                uri: url,
                body,
                headers: headers.clone(),
                imp_ids: imp_ids(&req.imp),
            });
        }
        (requests, errs)
    }

    fn make_bids(
        &self,
        request: &BidRequest,
        _request_data: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        match response.status_code {
            204 => return (None, vec![]),
            400 => {
                return (
                    None,
                    vec![BidderError::bad_input(format!("ERR, bad input {}", response.status_code))],
                )
            }
            200 => {}
            c => {
                return (None, vec![BidderError::bad_server_response(format!("ERR, response with status {c}"))])
            }
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut bid_response = BidderResponse::new();
        bid_response.currency = bid_resp.cur;
        for seat_bid in bid_resp.seatbid {
            for bid in seat_bid.bid {
                let t = get_media_type(&bid.impid, &request.imp);
                bid_response.bids.push(TypedBid::new(bid, t));
            }
        }
        (Some(bid_response), vec![])
    }
}

fn get_media_type(imp_id: &str, imps: &[Imp]) -> BidType {
    for imp in imps {
        if imp.id == imp_id {
            if imp.video.is_some() {
                return BidType::Video;
            }
            if imp.native.is_some() {
                return BidType::Native;
            }
            return BidType::Banner;
        }
    }
    BidType::Banner
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

/// Go `template.New("endpointTemplate").Parse(endpoint)`. Go rejects an undefined function
/// (`{{Malformed}}`) at parse time; the shared macros module only fails when resolving, so the
/// template is trial-resolved here to surface that.
fn parse_template(endpoint: &str) -> Result<EndpointTemplate, String> {
    let t = EndpointTemplate::parse(endpoint)?;
    if let Err(e) = t.resolve(&EndpointTemplateParams::default()) {
        if !e.contains("function \".") {
            return Err(e);
        }
    }
    Ok(t)
}
