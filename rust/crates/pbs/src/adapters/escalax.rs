//! Go `adapters/escalax/escalax.go`.

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
struct ImpExtParams {
    #[serde(rename = "accountId", default)]
    account_id: String,
    #[serde(rename = "sourceId", default)]
    source_id: String,
}

pub struct Adapter {
    endpoint: EndpointTemplate,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl AsRef<str>) -> Result<Self, BidderError> {
        let endpoint = parse_template(endpoint.as_ref())
            .map_err(|e| BidderError::other(format!("unable to parse endpoint url template: {e}")))?;
        Ok(Self { endpoint })
    }
}

fn get_headers(request: &BidRequest) -> Header {
    let mut headers = Header::new();
    headers.add("Content-Type", "application/json;charset=utf-8");
    headers.add("Accept", "application/json");
    headers.add("X-Openrtb-Version", "2.5");
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
    headers
}

fn get_impression_ext(imp: &Imp) -> Result<ImpExtParams, BidderError> {
    let bidder = imp_bidder_raw(&imp.ext).map_err(|e| BidderError::bad_input(format!("Error parsing escalaxExt - {e}")))?;
    unmarshal_raw::<ImpExtParams>(&bidder).map_err(|e| BidderError::bad_input(format!("Error parsing bidderExt - {e}")))
}

impl Bidder for Adapter {
    fn make_requests(&self, request: &BidRequest, _req_info: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        // Go indexes `Imp[0]` and panics on an empty imp list; report it instead.
        let Some(first) = request.imp.first() else {
            return (vec![], vec![BidderError::other("no imps in the bid request")]);
        };
        let ext = match get_impression_ext(first) {
            Ok(e) => e,
            Err(e) => return (vec![], vec![e]),
        };
        let params = EndpointTemplateParams { account_id: ext.account_id, source_id: ext.source_id, ..Default::default() };
        let url = match self.endpoint.resolve(&params) {
            Ok(u) => u,
            Err(e) => return (vec![], vec![BidderError::other(e)]),
        };
        // Go clears Imp[0].Ext on the shared request; do it on a clone.
        let mut req = request.clone();
        req.imp[0].ext = None;
        let body = match marshal(&req) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![e]),
        };
        (
            vec![RequestData {
                method: "POST".into(),
                uri: url,
                body,
                headers: get_headers(&req),
                imp_ids: imp_ids(&req.imp),
            }],
            vec![],
        )
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
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(_) => return (None, vec![BidderError::bad_server_response("Bad Server Response")]),
        };
        if bid_resp.seatbid.is_empty() {
            return (None, vec![BidderError::bad_server_response("Empty SeatBid array")]);
        }
        let mut bid_response = BidderResponse::with_bids_capacity(5);
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let t = match bid.mtype {
                    MarkupType::BANNER => BidType::Banner,
                    MarkupType::VIDEO => BidType::Video,
                    MarkupType::NATIVE => BidType::Native,
                    other => return (None, vec![BidderError::bad_input(format!("unsupported MType {}", other.0))]),
                };
                bid_response.bids.push(TypedBid::new(bid, t));
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
