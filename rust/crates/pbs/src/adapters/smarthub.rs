//! Go `adapters/smarthub/smarthub.go`.
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

const ADAPTER_VER: &str = "1.0.0";

pub struct Adapter {
    endpoint: EndpointTemplate,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl AsRef<str>) -> Result<Self, String> {
        let endpoint = parse_template(endpoint.as_ref())
            .map_err(|e| format!("unable to parse endpoint url template: {e}"))?;
        Ok(Self { endpoint })
    }
}

#[derive(serde::Deserialize, Default)]
struct ExtSmartHub {
    #[serde(rename = "partnerName", default)]
    partner_name: String,
    #[serde(default)]
    seat: String,
    #[serde(default)]
    token: String,
}

#[derive(serde::Deserialize, Default)]
struct SmartHubBidExt {
    #[serde(rename = "mediaType", default)]
    media_type: String,
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        // Go indexes `Imp[0]` and panics on an empty imp list; report it instead.
        let Some(first) = request.imp.first() else {
            return (vec![], vec![BidderError::bad_input("Bidder extension not provided or can't be unmarshalled")]);
        };
        let ext: ExtSmartHub = match parse_imp_ext(
            first,
            |_| BidderError::bad_input("Bidder extension not provided or can't be unmarshalled"),
            |_| BidderError::bad_input("Error while unmarshaling bidder extension"),
        ) {
            Ok(e) => e,
            Err(e) => return (vec![], vec![e]),
        };
        let params = EndpointTemplateParams {
            account_id: ext.seat,
            source_id: ext.token,
            host: ext.partner_name,
            ..Default::default()
        };
        let url = match self.endpoint.resolve(&params) {
            Ok(u) => u,
            Err(e) => return (vec![], vec![BidderError::other(e)]),
        };
        let body = match crate::go_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::other(e.to_string())]),
        };
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        headers.add("Prebid-Adapter-Ver", ADAPTER_VER);
        (
            vec![RequestData {
                method: "POST".into(),
                uri: url,
                body,
                headers,
                imp_ids: request.imp.iter().map(|i| i.id.clone()).collect(),
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
        let status = response_data.status_code;
        if status == 400 {
            return (
                None,
                vec![BidderError::bad_input(format!(
                    "Bad Request. {}",
                    String::from_utf8_lossy(&response_data.body)
                ))],
            );
        }
        if status == 503 {
            return (None, vec![BidderError::bad_input("Bidder unavailable. Please contact the bidder support.")]);
        }
        if status != 200 {
            return (
                None,
                vec![BidderError::bad_server_response(format!(
                    "Status Code: [ {status} ] {}",
                    String::from_utf8_lossy(&response_data.body)
                ))],
            );
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let Some(first_seat) = bid_resp.seatbid.into_iter().next() else {
            return (None, vec![BidderError::bad_server_response("Array SeatBid cannot be empty")]);
        };
        let mut bid_response = BidderResponse::with_bids_capacity(1);
        let Some(bid) = first_seat.bid.into_iter().next() else {
            return (None, vec![BidderError::bad_server_response("Array SeatBid[0].Bid cannot be empty")]);
        };
        let bid_ext: SmartHubBidExt = match unmarshal_ext(bid.ext.as_ref()) {
            Ok(e) => e,
            Err(_) => return (None, vec![BidderError::bad_server_response("Field BidExt is required")]),
        };
        let bid_type = match BidType::parse(&bid_ext.media_type) {
            Ok(t) => t,
            Err(e) => return (None, vec![BidderError::other(e)]),
        };
        bid_response.bids.push(TypedBid::new(bid, bid_type));
        (Some(bid_response), vec![])
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
