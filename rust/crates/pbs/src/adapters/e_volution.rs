//! Go `adapters/e_volution/evolution.go`.

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
struct BidExt {
    #[serde(rename = "mediaType", default)]
    media_type: String,
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

impl Bidder for Adapter {
    fn make_requests(&self, request: &BidRequest, _req_info: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let body = match marshal(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![e]),
        };
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        (
            vec![RequestData { method: "POST".into(), uri: self.uri.clone(), body, headers, imp_ids: imp_ids(&request.imp) }],
            vec![],
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
                    vec![BidderError::bad_input(format!("Bad Request. {}", String::from_utf8_lossy(&response.body)))],
                )
            }
            503 => return (None, vec![]),
            200 => {}
            c => {
                return (
                    None,
                    vec![BidderError::bad_server_response(format!(
                        "Something went wrong, please contact your Account Manager. Status Code: [ {c} ] "
                    ))],
                )
            }
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![BidderError::bad_server_response(format!("Bad response, {e}"))]),
        };
        let Some(sb) = bid_resp.seatbid.into_iter().next() else {
            return (None, vec![BidderError::bad_server_response("Empty seatbid")]);
        };
        let mut bid_response = BidderResponse::with_bids_capacity(sb.bid.len());
        let mut errs = vec![];
        for bid in sb.bid {
            let parsed: Result<BidExt, _> = unmarshal_raw(&ext_bytes(&bid.ext));
            let media = match parsed {
                Err(_) => "banner".to_string(),
                Ok(e) => e.media_type,
            };
            // Go keeps whatever string the bid ext carries as the bid type; an invalid one is
            // reported here instead of being passed on.
            match BidType::parse(&media) {
                Ok(t) => bid_response.bids.push(TypedBid::new(bid, t)),
                Err(e) => errs.push(BidderError::bad_server_response(e)),
            }
        }
        (Some(bid_response), errs)
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
