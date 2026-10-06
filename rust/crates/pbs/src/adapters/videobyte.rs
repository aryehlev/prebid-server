//! Go `adapters/videobyte/videobyte.go`.
#![allow(dead_code, unused_imports)]

use crate::bid_types::BidType;
use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder, BidderResponse,
    ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{Bid, BidRequest, BidResponse, Imp};
use crate::ortb::Ext;
use crate::macros::{EndpointTemplate, EndpointTemplateParams};

pub struct Adapter {
    endpoint: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self { endpoint: endpoint.into() }
    }
}

/// Go `openrtb_ext.ExtImpVideoByte`.
#[derive(serde::Deserialize, Default)]
struct ExtImpVideoByte {
    #[serde(rename = "pubId", default)]
    publisher_id: String,
    #[serde(rename = "placementId", default)]
    placement_id: String,
    #[serde(rename = "nid", default)]
    network_id: String,
}

fn parse_ext(imp: &Imp) -> Result<ExtImpVideoByte, BidderError> {
    parse_imp_ext(
        imp,
        |e| {
            BidderError::bad_input(format!(
                "Ignoring imp id={}, error while decoding extImpBidder, err: {e}",
                imp.id
            ))
        },
        |e| BidderError::bad_input(format!("Ignoring imp id={}, error while decoding impExt, err: {e}", imp.id)),
    )
}

/// Go `getParams(impExt).Encode()`: `url.Values.Encode` sorts by key.
fn get_params(ext: &ExtImpVideoByte) -> String {
    let mut params: Vec<(&str, &str)> = vec![("source", "pbs"), ("pid", &ext.publisher_id)];
    if !ext.placement_id.is_empty() {
        params.push(("placementId", &ext.placement_id));
    }
    if !ext.network_id.is_empty() {
        params.push(("nid", &ext.network_id));
    }
    params.sort_by(|a, b| a.0.cmp(b.0));
    params
        .iter()
        .map(|(k, v)| format!("{}={}", query_escape(k), query_escape(v)))
        .collect::<Vec<_>>()
        .join("&")
}

fn get_headers(request: &BidRequest) -> Header {
    let mut headers = Header::new();
    headers.add("Content-Type", "application/json;charset=utf-8");
    headers.add("Accept", "application/json");
    if let Some(site) = &request.site {
        if !site.domain.is_empty() {
            headers.add("Origin", site.domain.clone());
        }
        if !site.r#ref.is_empty() {
            headers.set("Referer", site.r#ref.clone());
        }
    }
    headers
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut adapter_requests = Vec::with_capacity(request.imp.len());
        let mut errs = vec![];
        let mut req = request.clone();
        for impression in &request.imp {
            let imp_ext = match parse_ext(impression) {
                Ok(e) => e,
                Err(e) => {
                    errs.push(e);
                    continue;
                }
            };
            req.imp = vec![impression.clone()];
            let body = match crate::go_json::to_vec(&req) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(BidderError::other(e.to_string()));
                    continue;
                }
            };
            adapter_requests.push(RequestData {
                method: "POST".into(),
                uri: format!("{}?{}", self.endpoint, get_params(&imp_ext)),
                body,
                headers: get_headers(&req),
                imp_ids: req.imp.iter().map(|i| i.id.clone()).collect(),
            });
        }
        (adapter_requests, errs)
    }

    fn make_bids(
        &self,
        internal_request: &BidRequest,
        _external_request: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if is_response_status_code_no_content(response) {
            return (None, vec![]);
        }
        if response.status_code == 400 {
            return (
                None,
                vec![BidderError::bad_input(format!(
                    "Bad user input: HTTP status {}. Run with request.debug = 1 for more info",
                    response.status_code
                ))],
            );
        }
        if response.status_code != 200 {
            return (
                None,
                vec![BidderError::bad_server_response(format!(
                    "Unexpected status code: {}. Run with request.debug = 1 for more info",
                    response.status_code
                ))],
            );
        }
        let ortb_response: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(_) => return (None, vec![BidderError::bad_server_response("Bad Server Response")]),
        };
        let mut bidder_response = BidderResponse::with_bids_capacity(1);
        for seat_bid in ortb_response.seatbid {
            for bid in seat_bid.bid {
                // Go keeps the last imp for a duplicated id (map overwrite).
                let imp = internal_request.imp.iter().rev().find(|i| i.id == bid.impid);
                let t = if imp.map_or(false, |i| i.banner.is_some()) { BidType::Banner } else { BidType::Video };
                bidder_response.bids.push(TypedBid::new(bid, t));
            }
        }
        (Some(bidder_response), vec![])
    }
}

// ── local helpers (Go `adapters.ExtImpBidder`, `openrtb_ext.ExtBid`) ─────────────────────────

/// Go `adapters.ExtImpBidder` (only `bidder` is used here).
#[derive(serde::Deserialize, Default)]
struct ExtImpBidder {
    #[serde(default)]
    bidder: Option<Ext>,
}

/// Go `jsonutil.Unmarshal(raw, &v)` on a `json.RawMessage` field. An absent message is empty
/// input, which json-iterator rejects with the same `expect { or n, but found` text.
fn unmarshal_ext<T: serde::de::DeserializeOwned>(ext: Option<&Ext>) -> Result<T, BidderError> {
    match ext {
        Some(e) => jsonutil::unmarshal(e.to_json().as_bytes()),
        None => Err(BidderError::FailedToUnmarshal("expect { or n, but found \u{0}".to_string())),
    }
}

/// Go: unmarshal `imp.ext` into `ExtImpBidder`, then `.Bidder` into `T`; the two errors are
/// returned separately so adapters can wrap them differently.
fn parse_imp_ext<T: serde::de::DeserializeOwned>(
    imp: &Imp,
    wrap_bidder_ext: impl Fn(BidderError) -> BidderError,
    wrap_params: impl Fn(BidderError) -> BidderError,
) -> Result<T, BidderError> {
    let bidder_ext: ExtImpBidder = unmarshal_ext(imp.ext.as_ref()).map_err(wrap_bidder_ext)?;
    unmarshal_ext(bidder_ext.bidder.as_ref()).map_err(wrap_params)
}

/// Go `openrtb_ext.ExtBid` (`bid.ext.prebid.type`).
#[derive(serde::Deserialize, Default)]
struct ExtBid {
    #[serde(default)]
    prebid: Option<ExtBidPrebid>,
}

#[derive(serde::Deserialize, Default)]
struct ExtBidPrebid {
    #[serde(rename = "type", default)]
    bid_type: String,
}

/// Go `jsonutil.Unmarshal(bid.Ext, &ExtBid)`: `Err` on a malformed ext.
fn parse_bid_ext(bid: &Bid) -> Option<Result<ExtBid, BidderError>> {
    bid.ext.as_ref().map(|e| jsonutil::unmarshal(e.to_json().as_bytes()))
}

/// Go `strconv.FormatFloat(price, 'f', -1, 64)`.
fn format_price(price: f64) -> String {
    format!("{price}")
}

/// Go `template.New("endpointTemplate").Parse(endpoint)`. `{{Malformed}}` is a Go parse error
/// (unknown function), which `EndpointTemplate::parse` only reports at resolve time, so a dry
/// run with empty params catches it here.
fn parse_template(endpoint: &str) -> Result<EndpointTemplate, String> {
    let t = EndpointTemplate::parse(endpoint)?;
    t.resolve(&EndpointTemplateParams::default())?;
    Ok(t)
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

/// Go `url.PathEscape`.
fn path_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'$' | b'&' | b'+'
            | b'=' | b':' | b'@' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
