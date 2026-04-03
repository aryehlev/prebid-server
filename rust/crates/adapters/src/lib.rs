use std::collections::HashMap;

use openrtb_ext::{BidType, ExtBidPrebidMeta, ExtBidPrebidVideo, FledgeAuctionConfig};

pub mod adapters;
pub mod adapters2;
pub mod adapters3;
pub mod adapters4;
pub mod adapters5;
pub mod adapters6;
pub mod adapters7;
pub mod adapters8;
pub mod adapters9;
pub mod adapters10;
pub mod adapters11;
pub mod adapters12;
pub mod adapters13;
pub mod adapters14;
pub mod registry;

/// RequestData packages together the fields needed to make an HTTP request to a bidder.
#[derive(Debug, Clone, Default)]
pub struct RequestData {
    pub method: String,
    pub uri: String,
    pub body: Vec<u8>,
    pub headers: HashMap<String, String>,
    pub imp_ids: Vec<String>,
}

impl RequestData {
    pub fn new(method: impl Into<String>, uri: impl Into<String>, body: Vec<u8>) -> Self {
        Self {
            method: method.into(),
            uri: uri.into(),
            body,
            headers: HashMap::new(),
            imp_ids: Vec::new(),
        }
    }

    pub fn new_post(uri: impl Into<String>, body: Vec<u8>) -> Self {
        let mut headers = HashMap::new();
        headers.insert(
            "Content-Type".to_string(),
            "application/json;charset=utf-8".to_string(),
        );
        headers.insert("Accept".to_string(), "application/json".to_string());
        Self {
            method: "POST".to_string(),
            uri: uri.into(),
            body,
            headers,
            imp_ids: Vec::new(),
        }
    }

    pub fn set_header(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.headers.insert(key.into(), value.into());
    }

    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }
}

/// ResponseData packages together information from a bidder's HTTP response.
#[derive(Debug, Clone, Default)]
pub struct ResponseData {
    pub status_code: u16,
    pub body: Vec<u8>,
    pub headers: HashMap<String, String>,
}

impl ResponseData {
    pub fn new(status_code: u16, body: Vec<u8>) -> Self {
        Self {
            status_code,
            body,
            headers: HashMap::new(),
        }
    }
}

/// TypedBid packages a bid with bidder-specific metadata that prebid-server needs.
#[derive(Debug, Clone)]
pub struct TypedBid {
    pub bid: openrtb::Bid,
    pub bid_meta: Option<ExtBidPrebidMeta>,
    pub bid_type: BidType,
    pub bid_video: Option<ExtBidPrebidVideo>,
    pub deal_priority: i32,
    pub orig_bid_cpm: f64,
    pub orig_bid_cur: String,
    pub orig_bid_cpm_usd: f64,
}

impl TypedBid {
    pub fn new(bid: openrtb::Bid, bid_type: BidType) -> Self {
        Self {
            bid,
            bid_type,
            bid_meta: None,
            bid_video: None,
            deal_priority: 0,
            orig_bid_cpm: 0.0,
            orig_bid_cur: "USD".to_string(),
            orig_bid_cpm_usd: 0.0,
        }
    }
}

/// BidderResponse wraps the bidder's response with bids and currency.
#[derive(Debug, Clone)]
pub struct BidderResponse {
    pub currency: String,
    pub bids: Vec<TypedBid>,
    pub fledge_auction_configs: Vec<FledgeAuctionConfig>,
}

impl BidderResponse {
    pub fn new() -> Self {
        Self {
            currency: "USD".to_string(),
            bids: Vec::new(),
            fledge_auction_configs: Vec::new(),
        }
    }

    pub fn with_capacity(cap: usize) -> Self {
        Self {
            currency: "USD".to_string(),
            bids: Vec::with_capacity(cap),
            fledge_auction_configs: Vec::new(),
        }
    }
}

impl Default for BidderResponse {
    fn default() -> Self {
        Self::new()
    }
}

/// ExtraRequestInfo contains extra information available when making bidder requests.
#[derive(Debug, Clone, Default)]
pub struct ExtraRequestInfo {
    pub pbs_entry_point: String,
    pub global_privacy_control_header: String,
}

/// BidderError represents errors that can occur in bidder adapters.
#[derive(Debug, Clone, thiserror::Error)]
pub enum BidderError {
    #[error("bad input: {0}")]
    BadInput(String),
    #[error("bad server response: {0}")]
    BadServerResponse(String),
    #[error("failed to request bids: {0}")]
    FailedToRequestBids(String),
    #[error("timeout")]
    Timeout,
    #[error("{0}")]
    Unknown(String),
}

impl BidderError {
    pub fn bad_input(msg: impl Into<String>) -> Self {
        BidderError::BadInput(msg.into())
    }

    pub fn bad_server_response(msg: impl Into<String>) -> Self {
        BidderError::BadServerResponse(msg.into())
    }

    pub fn is_fatal(&self) -> bool {
        matches!(self, BidderError::BadInput(_))
    }
}

/// Bidder describes how to connect to external demand.
/// All bidder adapters must implement this trait.
pub trait Bidder: Send + Sync {
    /// Make the HTTP requests to fetch bids for the given bid request.
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>);

    /// Unpack the server's response into typed bids.
    fn make_bids(
        &self,
        internal_request: &openrtb::BidRequest,
        external_request: &RequestData,
        response: &ResponseData,
    ) -> Result<BidderResponse, Vec<BidderError>>;
}

/// TimeoutBidder is implemented by bidders that support timeout notifications.
pub trait TimeoutBidder: Bidder {
    /// Create a timeout notification request based on the timed-out request.
    fn make_timeout_notification(&self, req: &RequestData) -> Result<RequestData, BidderError>;
}

/// Builder function type for constructing bidder instances.
pub type BidderBuilder = fn(name: &str, endpoint: &str) -> anyhow::Result<Box<dyn Bidder>>;

/// Helper to get imp IDs from a slice of impressions.
pub fn get_imp_ids(imps: &[openrtb::Imp]) -> Vec<String> {
    imps.iter().map(|imp| imp.id.clone()).collect()
}

/// Helper to determine bid type from an impression.
pub fn get_bid_type_from_imp(imp: &openrtb::Imp) -> BidType {
    if imp.video.is_some() {
        BidType::Video
    } else if imp.native.is_some() {
        BidType::Native
    } else if imp.audio.is_some() {
        BidType::Audio
    } else {
        BidType::Banner
    }
}

/// Standard HTTP response status handling used by adapters.
pub fn check_response_status(status_code: u16) -> Result<(), BidderError> {
    match status_code {
        200 => Ok(()),
        204 => Err(BidderError::Unknown("no content".to_string())),
        400 => Err(BidderError::BadInput(format!(
            "Unexpected status code: {status_code}"
        ))),
        _ => Err(BidderError::BadServerResponse(format!(
            "Unexpected status code: {status_code}"
        ))),
    }
}

/// Map OpenRTB 2.6 mtype to BidType. 0/unknown → Banner.
pub fn get_bid_type_from_mtype(mtype: i32) -> openrtb_ext::BidType {
    match mtype {
        2 => openrtb_ext::BidType::Video,
        3 => openrtb_ext::BidType::Audio,
        4 => openrtb_ext::BidType::Native,
        _ => openrtb_ext::BidType::Banner,
    }
}
