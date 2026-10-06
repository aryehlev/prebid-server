//! Go `adapters/aso/aso.go`.
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
    endpoint: EndpointTemplate,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl AsRef<str>) -> Result<Self, String> {
        let endpoint = parse_template(endpoint.as_ref())
            .map_err(|e| format!("unable to parse endpoint template: {e}"))?;
        Ok(Self { endpoint })
    }
}

#[derive(serde::Deserialize, Default)]
struct ExtImpAso {
    #[serde(default)]
    zone: i64,
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
        for imp in &request.imp {
            let bidder_ext: ExtImpBidder = match unmarshal_ext(imp.ext.as_ref()) {
                Ok(e) => e,
                Err(e) => {
                    errors.push(BidderError::bad_input(format!("invalid imp.ext, {e}")));
                    continue;
                }
            };
            let imp_ext: ExtImpAso = match unmarshal_ext(bidder_ext.bidder.as_ref()) {
                Ok(e) => e,
                Err(e) => {
                    errors.push(BidderError::bad_input(format!("invalid bidderExt.Bidder, {e}")));
                    continue;
                }
            };
            request_copy.imp = vec![imp.clone()];
            let endpoint = match self
                .endpoint
                .resolve(&EndpointTemplateParams { zone_id: imp_ext.zone.to_string(), ..Default::default() })
            {
                Ok(u) => u,
                Err(e) => {
                    errors.push(BidderError::other(e));
                    continue;
                }
            };
            let req_json = match crate::go_json::to_vec(&request_copy) {
                Ok(b) => b,
                Err(e) => {
                    errors.push(BidderError::other(e.to_string()));
                    continue;
                }
            };
            let mut headers = Header::new();
            headers.add("Content-Type", "application/json;charset=utf-8");
            headers.add("Accept", "application/json");
            requests.push(RequestData {
                method: "POST".into(),
                uri: endpoint,
                body: req_json,
                headers,
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
        if let Some(e) = check_response_status_code_for_errors(response_data) {
            return (None, vec![e]);
        }
        let response: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut bid_response = BidderResponse::with_bids_capacity(request.imp.len());
        bid_response.currency = response.cur.clone();
        let mut errors = vec![];
        for seat_bid in response.seatbid {
            for mut bid in seat_bid.bid {
                resolve_macros(&mut bid);
                match get_media_type(&bid) {
                    Ok(t) => bid_response.bids.push(TypedBid::new(bid, t)),
                    Err(e) => errors.push(e),
                }
            }
        }
        (Some(bid_response), errors)
    }
}

fn get_media_type(bid: &Bid) -> Result<BidType, BidderError> {
    if let Some(Ok(ext)) = parse_bid_ext(bid) {
        if let Some(prebid) = ext.prebid {
            return BidType::parse(&prebid.bid_type).map_err(BidderError::other);
        }
    }
    Err(BidderError::bad_server_response(format!("Failed to get type of bid \"{}\"", bid.impid)))
}

fn resolve_macros(bid: &mut Bid) {
    let price = format_price(bid.price);
    bid.nurl = bid.nurl.replace("${AUCTION_PRICE}", &price);
    bid.adm = bid.adm.replace("${AUCTION_PRICE}", &price);
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
