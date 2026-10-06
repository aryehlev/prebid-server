//! Go `adapters/bidder.go`: the `Bidder` interface and the data it passes around.

use crate::bid_types::{BidType, ExtBidPrebidMeta, ExtBidPrebidVideo};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::ortb::openrtb2::{Bid, BidRequest};
use crate::ortb::Ext;

/// Go `adapters.RequestData`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RequestData {
    pub method: String,
    pub uri: String,
    pub body: Vec<u8>,
    pub headers: Header,
    pub imp_ids: Vec<String>,
}

impl RequestData {
    /// Go `RequestData.SetBasicAuth`.
    pub fn set_basic_auth(&mut self, username: &str, password: &str) {
        use base64::Engine as _;
        let token = base64::engine::general_purpose::STANDARD.encode(format!("{username}:{password}"));
        self.headers.set("Authorization", format!("Basic {token}"));
    }
}

/// Go `adapters.ResponseData`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResponseData {
    pub status_code: u16,
    pub body: Vec<u8>,
    pub headers: Header,
}

/// Go `adapters.TypedBid`.
#[derive(Debug, Clone, PartialEq)]
pub struct TypedBid {
    pub bid: Bid,
    pub bid_meta: Option<ExtBidPrebidMeta>,
    pub bid_type: BidType,
    pub bid_video: Option<ExtBidPrebidVideo>,
    pub deal_priority: i32,
    /// New seat the bid is placed under; empty means the adapter name.
    pub seat: String,
}

impl TypedBid {
    pub fn new(bid: Bid, bid_type: BidType) -> Self {
        Self { bid, bid_meta: None, bid_type, bid_video: None, deal_priority: 0, seat: String::new() }
    }
}

/// Go `adapters.BidderResponse`.
#[derive(Debug, Clone, PartialEq)]
pub struct BidderResponse {
    pub currency: String,
    pub bids: Vec<TypedBid>,
    pub fledge_auction_configs: Option<Ext>,
}

impl BidderResponse {
    /// Go `NewBidderResponse` (currency `USD`).
    pub fn new() -> Self {
        Self::with_bids_capacity(0)
    }

    /// Go `NewBidderResponseWithBidsCapacity`.
    pub fn with_bids_capacity(capacity: usize) -> Self {
        Self { currency: "USD".to_string(), bids: Vec::with_capacity(capacity), fledge_auction_configs: None }
    }
}

impl Default for BidderResponse {
    fn default() -> Self {
        Self::new()
    }
}

/// Go `adapters.ExtraRequestInfo`.
#[derive(Debug, Clone, Default)]
pub struct ExtraRequestInfo {
    pub pbs_entry_point: String,
    pub global_privacy_control_header: String,
    pub currency_conversions: crate::currency::Conversions,
    pub preferred_media_type: Option<BidType>,
}

impl ExtraRequestInfo {
    /// Go `ExtraRequestInfo.ConvertCurrency`.
    pub fn convert_currency(&self, value: f64, from: &str, to: &str) -> Result<f64, BidderError> {
        self.currency_conversions.get_rate(from, to).map(|rate| value * rate)
    }
}

/// Go `adapters.Bidder`.
///
/// `request` has only valid `{Imp.Type, Platform}` combinations and `imp.ext` of the form
/// `{"bidder": params}`; the adapter reads it straight from the typed [`BidRequest`].
pub trait Bidder: Send + Sync {
    fn make_requests(
        &self,
        request: &BidRequest,
        req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>);

    fn make_bids(
        &self,
        internal_request: &BidRequest,
        external_request: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>);
}

/// Go `adapters.TimeoutBidder`.
pub trait TimeoutBidder: Bidder {
    fn make_timeout_notification(&self, req: &RequestData) -> Result<RequestData, BidderError>;
}

/// Go `adapters.IsResponseStatusCodeNoContent`.
pub fn is_response_status_code_no_content(response: &ResponseData) -> bool {
    response.status_code == 204
}

/// Go `adapters.CheckResponseStatusCodeForErrors`: 400 is `BadInput`, any other non-200 status
/// is `BadServerResponse`.
pub fn check_response_status_code_for_errors(response: &ResponseData) -> Option<BidderError> {
    let msg = || {
        format!(
            "Unexpected status code: {}. Run with request.debug = 1 for more info",
            response.status_code
        )
    };
    match response.status_code {
        400 => Some(BidderError::bad_input(msg())),
        200 => None,
        _ => Some(BidderError::bad_server_response(msg())),
    }
}
