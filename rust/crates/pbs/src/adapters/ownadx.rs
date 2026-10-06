//! Go `adapters/ownadx/ownadx.go`.
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

use crate::ortb::openrtb2::MarkupType;

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

/// Go `openrtb_ext.ExtImpOwnAdx` (also the grouping key).
#[derive(serde::Deserialize, Default, Clone, PartialEq, Eq)]
struct ExtImpOwnAdx {
    #[serde(rename = "sspId", default)]
    ssp_id: String,
    #[serde(rename = "seatId", default)]
    seat_id: String,
    #[serde(rename = "tokenId", default)]
    token_id: String,
}

fn get_impression_ext(imp: &Imp) -> Result<ExtImpOwnAdx, BidderError> {
    parse_imp_ext(
        imp,
        |_| BidderError::bad_input("Bidder extension not valid or can't be unmarshalled"),
        |_| BidderError::bad_input("Error while unmarshaling bidder extension"),
    )
}

impl Adapter {
    fn get_request_data(
        &self,
        bid_request: &BidRequest,
        imp_ext: &ExtImpOwnAdx,
        imps: Vec<Imp>,
    ) -> Result<RequestData, BidderError> {
        let mut pbid_request = bid_request.clone();
        pbid_request.imp = imps;
        let body = crate::go_json::to_vec(&pbid_request).map_err(|e| {
            BidderError::bad_input(format!("Prebid bidder request not valid or can't be marshalled. Err: {e}"))
        })?;
        let params = EndpointTemplateParams {
            ssp_id: imp_ext.ssp_id.clone(),
            seat_id: imp_ext.seat_id.clone(),
            token_id: imp_ext.token_id.clone(),
            ..Default::default()
        };
        let url = self
            .endpoint
            .resolve(&params)
            .map_err(|e| BidderError::bad_input(format!("Error while creating endpoint. Err: {e}")))?;
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        headers.add("x-openrtb-version", "2.5");
        Ok(RequestData {
            method: "POST".into(),
            uri: url,
            body,
            headers,
            imp_ids: pbid_request.imp.iter().map(|i| i.id.clone()).collect(),
        })
    }
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = vec![];
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::bad_input("No impression in the bid request")]);
        }
        // Go groups into a map (random iteration order); first-seen order is kept here.
        let mut groups: Vec<(ExtImpOwnAdx, Vec<Imp>)> = vec![];
        for imp in &request.imp {
            match get_impression_ext(imp) {
                Ok(ext) => match groups.iter_mut().find(|(k, _)| *k == ext) {
                    Some((_, imps)) => imps.push(imp.clone()),
                    None => groups.push((ext, vec![imp.clone()])),
                },
                Err(e) => errs.push(e),
            }
        }
        if groups.is_empty() {
            return (vec![], errs);
        }
        let mut req_detail = Vec::with_capacity(groups.len());
        for (k, imps) in groups {
            match self.get_request_data(request, &k, imps) {
                Ok(r) => req_detail.push(r),
                Err(e) => errs.push(e),
            }
        }
        (req_detail, errs)
    }

    fn make_bids(
        &self,
        _internal_request: &BidRequest,
        _external_request: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if is_response_status_code_no_content(response) {
            return (None, vec![]);
        }
        if response.status_code == 400 {
            return (
                None,
                vec![BidderError::bad_server_response(format!("Bad request: {}", response.status_code))],
            );
        }
        if response.status_code != 200 {
            return (
                None,
                vec![BidderError::bad_server_response(format!(
                    "Unexpected status code: {}. Run with request.test = 1 for more info.",
                    response.status_code
                ))],
            );
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(_) => return (None, vec![BidderError::bad_server_response("Bad server response ")]),
        };
        let Some(seat_bid) = bid_resp.seatbid.into_iter().next() else {
            return (None, vec![BidderError::bad_server_response("Array SeatBid cannot be empty ")]);
        };
        let mut bid_response = BidderResponse::with_bids_capacity(seat_bid.bid.len());
        if seat_bid.bid.is_empty() {
            return (None, vec![BidderError::bad_server_response("Bid cannot be empty ")]);
        }
        for bid in seat_bid.bid {
            let bid_type = match bid.mtype {
                MarkupType::BANNER => BidType::Banner,
                MarkupType::VIDEO => BidType::Video,
                MarkupType::AUDIO => BidType::Audio,
                MarkupType::NATIVE => BidType::Native,
                _ => return (None, vec![BidderError::bad_server_response("Bid type is invalid")]),
            };
            bid_response.bids.push(TypedBid::new(bid, bid_type));
        }
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
