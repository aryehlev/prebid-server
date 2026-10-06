//! Go `adapters/adtrgtme/adtrgtme.go`.
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

#[derive(serde::Deserialize, Default)]
struct ExtImpAdtrgtme {
    #[serde(default)]
    site_id: u64,
}

fn get_site_id_from_imp(imp: &Imp) -> Result<u64, BidderError> {
    let ext: ExtImpAdtrgtme = parse_imp_ext(
        imp,
        |_| BidderError::bad_input("ext.bidder not provided"),
        |_| BidderError::bad_input("ext.bidder not provided"),
    )?;
    Ok(ext.site_id)
}

fn make_request_headers(request: &BidRequest) -> Header {
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

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = vec![];
        let mut errors = vec![];
        let mut request_copy = request.clone();
        for src in &request.imp {
            let site_id = match get_site_id_from_imp(src) {
                Ok(s) => s,
                Err(e) => {
                    errors.push(e);
                    continue;
                }
            };
            let mut imp = src.clone();
            imp.ext = None;
            request_copy.imp = vec![imp];
            let body = match crate::go_json::to_vec(&request_copy) {
                Ok(b) => b,
                Err(e) => {
                    errors.push(BidderError::other(e.to_string()));
                    continue;
                }
            };
            requests.push(RequestData {
                method: "POST".into(),
                uri: format!("{}?s={}&prebid", self.endpoint, site_id),
                body,
                headers: make_request_headers(request),
                imp_ids: request_copy.imp.iter().map(|i| i.id.clone()).collect(),
            });
        }
        (requests, errors)
    }

    fn make_bids(
        &self,
        request: &BidRequest,
        _request_data: &RequestData,
        response_data: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if is_response_status_code_no_content(response_data) {
            return (None, vec![]);
        }
        let status = response_data.status_code;
        if status == 400 {
            return (None, vec![BidderError::bad_input(format!("Unexpected status code: [ {status} ]"))]);
        }
        if status == 503 {
            return (
                None,
                vec![BidderError::bad_input(format!(
                    "Something went wrong, please contact your Account Manager. Status Code: [ {status} ] "
                ))],
            );
        }
        if status != 200 {
            return (
                None,
                vec![BidderError::bad_input(format!(
                    "Unexpected status code: [ {status} ]. Run with request.debug = 1 for more info"
                ))],
            );
        }
        let response: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(_) => return (None, vec![BidderError::bad_server_response("Bad Server Response")]),
        };
        if response.seatbid.is_empty() {
            return (None, vec![BidderError::bad_server_response("Empty SeatBid array")]);
        }
        let mut bid_response = BidderResponse::with_bids_capacity(request.imp.len());
        bid_response.currency = response.cur.clone();
        for seat_bid in response.seatbid {
            for bid in seat_bid.bid {
                match get_media_type_for_imp(&bid.impid, &request.imp) {
                    Ok(t) => bid_response.bids.push(TypedBid::new(bid, t)),
                    Err(e) => return (None, vec![e]),
                }
            }
        }
        (Some(bid_response), vec![])
    }
}

fn get_media_type_for_imp(imp_id: &str, imps: &[Imp]) -> Result<BidType, BidderError> {
    for imp in imps {
        if imp.id == imp_id {
            if imp.banner.is_some() {
                return Ok(BidType::Banner);
            }
            return Err(BidderError::bad_input(format!("Unsupported bidtype for bid: \"{imp_id}\"")));
        }
    }
    Err(BidderError::bad_input(format!("Failed to find impression: \"{imp_id}\"")))
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
