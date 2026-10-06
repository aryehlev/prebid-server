//! Go `adapters/adot/adot.go`.
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
struct ExtImpAdot {
    #[serde(rename = "publisherPath", default)]
    publisher_path: String,
}

#[derive(serde::Deserialize, Default)]
struct AdotBidExt {
    #[serde(default)]
    adot: AdotMedia,
}

#[derive(serde::Deserialize, Default)]
struct AdotMedia {
    #[serde(default)]
    media_type: String,
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let body = match crate::go_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => {
                return (vec![], vec![BidderError::other(format!("unable to marshal openrtb request ({e})"))])
            }
        };
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");

        // Go indexes `Imp[0]` and panics on an empty imp list; treat it as no ext.
        let publisher_path = request
            .imp
            .first()
            .and_then(|imp| parse_imp_ext::<ExtImpAdot>(imp, |e| e, |e| e).ok())
            .map(|e| e.publisher_path)
            .unwrap_or_default();
        let endpoint = self.endpoint.replace("{PUBLISHER_PATH}", &publisher_path);

        let data = RequestData {
            method: "POST".into(),
            uri: endpoint,
            body,
            headers,
            imp_ids: request.imp.iter().map(|i| i.id.clone()).collect(),
        };
        (vec![data], vec![])
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
        let msg = || {
            format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info",
                response_data.status_code
            )
        };
        if response_data.status_code == 400 {
            return (None, vec![BidderError::bad_input(msg())]);
        }
        if response_data.status_code != 200 {
            return (None, vec![BidderError::bad_server_response(msg())]);
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let capacity = bid_resp.seatbid.first().map_or(1, |s| s.bid.len());
        let mut bid_response = BidderResponse::with_bids_capacity(capacity);
        for sb in bid_resp.seatbid {
            for mut bid in sb.bid {
                if let Some(t) = get_media_type_for_bid(&bid) {
                    let price = format_price(bid.price);
                    bid.nurl = bid.nurl.replace("${AUCTION_PRICE}", &price);
                    bid.adm = bid.adm.replace("${AUCTION_PRICE}", &price);
                    bid_response.bids.push(TypedBid::new(bid, t));
                }
            }
        }
        (Some(bid_response), vec![])
    }
}

/// Go `getMediaTypeForBid`: errors are swallowed by the caller, so `None` is the error.
fn get_media_type_for_bid(bid: &Bid) -> Option<BidType> {
    let ext: AdotBidExt = unmarshal_ext(bid.ext.as_ref()).ok()?;
    match ext.adot.media_type.as_str() {
        "banner" => Some(BidType::Banner),
        "video" => Some(BidType::Video),
        "native" => Some(BidType::Native),
        _ => None,
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
