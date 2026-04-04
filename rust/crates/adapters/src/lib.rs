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

#[cfg(test)]
mod tests {
    use super::*;
    use openrtb::{Banner, Format, Imp, Video};

    fn make_format(w: i32, h: i32) -> Format {
        Format {
            w: Some(w),
            h: Some(h),
            wratio: None,
            hratio: None,
            wmin: None,
            ext: None,
        }
    }

    fn imp_with_banner() -> Imp {
        Imp {
            id: "imp1".to_string(),
            banner: Some(Banner {
                format: Some(vec![make_format(300, 250)]),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn test_get_bid_type_from_imp_banner() {
        let imp = imp_with_banner();
        assert_eq!(get_bid_type_from_imp(&imp), openrtb_ext::BidType::Banner);
    }

    #[test]
    fn test_get_bid_type_from_imp_video() {
        let mut imp = imp_with_banner();
        imp.banner = None;
        imp.video = Some(Video::default());
        assert_eq!(get_bid_type_from_imp(&imp), openrtb_ext::BidType::Video);
    }

    #[test]
    fn test_get_bid_type_from_imp_native() {
        let mut imp = imp_with_banner();
        imp.banner = None;
        imp.native = Some(openrtb::Native::default());
        assert_eq!(get_bid_type_from_imp(&imp), openrtb_ext::BidType::Native);
    }

    #[test]
    fn test_get_bid_type_from_imp_audio() {
        let mut imp = imp_with_banner();
        imp.banner = None;
        imp.audio = Some(openrtb::Audio::default());
        assert_eq!(get_bid_type_from_imp(&imp), openrtb_ext::BidType::Audio);
    }

    #[test]
    fn test_get_bid_type_from_mtype() {
        assert_eq!(get_bid_type_from_mtype(1), openrtb_ext::BidType::Banner);
        assert_eq!(get_bid_type_from_mtype(2), openrtb_ext::BidType::Video);
        assert_eq!(get_bid_type_from_mtype(3), openrtb_ext::BidType::Audio);
        assert_eq!(get_bid_type_from_mtype(4), openrtb_ext::BidType::Native);
        assert_eq!(get_bid_type_from_mtype(0), openrtb_ext::BidType::Banner);
        assert_eq!(get_bid_type_from_mtype(99), openrtb_ext::BidType::Banner);
    }

    #[test]
    fn test_check_response_status() {
        assert!(check_response_status(200).is_ok());
        assert!(check_response_status(204).is_err());
        assert!(check_response_status(400).is_err());
        assert!(check_response_status(500).is_err());
        assert!(check_response_status(503).is_err());
    }

    #[test]
    fn test_get_imp_ids() {
        let imps = vec![
            Imp {
                id: "a".to_string(),
                ..Default::default()
            },
            Imp {
                id: "b".to_string(),
                ..Default::default()
            },
        ];
        let ids = get_imp_ids(&imps);
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"a".to_string()));
        assert!(ids.contains(&"b".to_string()));
    }

    #[test]
    fn test_get_imp_ids_empty() {
        let ids = get_imp_ids(&[]);
        assert!(ids.is_empty());
    }

    #[test]
    fn test_request_data_new_post_has_content_type() {
        let rd = RequestData::new_post("http://example.com", b"{}".to_vec());
        assert_eq!(rd.method, "POST");
        assert_eq!(rd.uri, "http://example.com");
        assert!(rd.headers.contains_key("Content-Type"));
    }

    #[test]
    fn test_request_data_with_header() {
        let rd = RequestData::new_post("http://example.com", vec![])
            .with_header("X-Custom", "value");
        assert_eq!(rd.headers.get("X-Custom").map(String::as_str), Some("value"));
    }

    #[test]
    fn test_response_data_new() {
        let rd = ResponseData::new(200, b"hello".to_vec());
        assert_eq!(rd.status_code, 200);
        assert_eq!(rd.body, b"hello");
    }

    #[test]
    fn test_typed_bid_new() {
        let bid = openrtb::Bid {
            id: "b1".to_string(),
            impid: "imp1".to_string(),
            price: 2.5,
            ..Default::default()
        };
        let tb = TypedBid::new(bid.clone(), openrtb_ext::BidType::Banner);
        assert_eq!(tb.bid.id, "b1");
        assert_eq!(tb.bid_type, openrtb_ext::BidType::Banner);
        assert_eq!(tb.orig_bid_cur, "USD");
    }

    #[test]
    fn test_bidder_response_new() {
        let resp = BidderResponse::new();
        assert_eq!(resp.currency, "USD");
        assert!(resp.bids.is_empty());
        assert!(resp.fledge_auction_configs.is_empty());
    }

    #[test]
    fn test_bidder_error_is_fatal() {
        let bad_input = BidderError::bad_input("nope");
        assert!(bad_input.is_fatal());

        let bad_server = BidderError::bad_server_response("nope");
        assert!(!bad_server.is_fatal());

        let timeout = BidderError::Timeout;
        assert!(!timeout.is_fatal());
    }

    // ── Appnexus ─────────────────────────────────────────────────────────────

    #[test]
    fn test_appnexus_make_requests_post_to_correct_url() {
        let adapter = crate::adapters::AppnexusAdapter::new(
            "https://ib.adnxs.com/openrtb2".to_string(),
        );
        let mut imp = openrtb::Imp::default();
        imp.id = "imp1".to_string();
        imp.banner = Some(Banner {
            format: Some(vec![make_format(300, 250)]),
            ..Default::default()
        });
        imp.ext = Some(serde_json::json!({"bidder": {"placement_id": 12345}}));
        let req = openrtb::BidRequest {
            id: "req1".to_string(),
            imp: vec![imp],
            ..Default::default()
        };
        let (requests, errors) = adapter.make_requests(&req, &ExtraRequestInfo::default());
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
        assert_eq!(requests.len(), 1);
        let rd = &requests[0];
        assert_eq!(rd.method, "POST");
        assert_eq!(rd.uri, "https://ib.adnxs.com/openrtb2");
        assert!(!rd.body.is_empty());
    }

    #[test]
    fn test_appnexus_make_requests_body_is_valid_json() {
        let adapter = crate::adapters::AppnexusAdapter::new(
            "https://ib.adnxs.com/openrtb2".to_string(),
        );
        let mut imp = openrtb::Imp::default();
        imp.id = "imp2".to_string();
        imp.banner = Some(Banner::default());
        imp.ext = Some(serde_json::json!({"bidder": {"placement_id": 99}}));
        let req = openrtb::BidRequest {
            id: "req2".to_string(),
            imp: vec![imp],
            ..Default::default()
        };
        let (requests, errors) = adapter.make_requests(&req, &ExtraRequestInfo::default());
        assert!(errors.is_empty());
        let parsed: serde_json::Value =
            serde_json::from_slice(&requests[0].body).expect("body should be valid JSON");
        assert!(parsed.get("imp").is_some());
    }

    #[test]
    fn test_appnexus_make_requests_member_id_in_url() {
        let adapter = crate::adapters::AppnexusAdapter::new(
            "https://ib.adnxs.com/openrtb2".to_string(),
        );
        let mut imp = openrtb::Imp::default();
        imp.id = "imp3".to_string();
        imp.banner = Some(Banner::default());
        imp.ext =
            Some(serde_json::json!({"bidder": {"placement_id": 12345, "member": "9999"}}));
        let req = openrtb::BidRequest {
            id: "req3".to_string(),
            imp: vec![imp],
            ..Default::default()
        };
        let (requests, errors) = adapter.make_requests(&req, &ExtraRequestInfo::default());
        assert!(errors.is_empty());
        assert_eq!(requests.len(), 1);
        assert!(
            requests[0].uri.contains("member_id=9999"),
            "URI should contain member_id param, got: {}",
            requests[0].uri
        );
    }

    // ── Rubicon ──────────────────────────────────────────────────────────────

    #[test]
    fn test_rubicon_make_requests_includes_authorization_header() {
        let adapter = crate::adapters::RubiconAdapter::new(
            "https://rubicon.example.com/openrtb2/auction".to_string(),
            "myuser".to_string(),
            "mypass".to_string(),
        );
        let mut imp = openrtb::Imp::default();
        imp.id = "imp1".to_string();
        imp.banner = Some(Banner::default());
        imp.ext = Some(
            serde_json::json!({"bidder": {"accountId": 1234, "siteId": 5678, "zoneId": 9012}}),
        );
        let req = openrtb::BidRequest {
            id: "rub-req".to_string(),
            imp: vec![imp],
            ..Default::default()
        };
        let (requests, errors) = adapter.make_requests(&req, &ExtraRequestInfo::default());
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
        assert_eq!(requests.len(), 1);
        let rd = &requests[0];
        assert_eq!(rd.method, "POST");
        assert!(
            rd.headers.contains_key("Authorization"),
            "Authorization header must be present"
        );
        let auth = rd.headers.get("Authorization").unwrap();
        assert!(auth.starts_with("Basic "), "should use Basic auth, got: {auth}");
    }

    #[test]
    fn test_rubicon_make_requests_posts_to_endpoint() {
        let endpoint = "https://rubicon.example.com/openrtb2/auction".to_string();
        let adapter = crate::adapters::RubiconAdapter::new(
            endpoint.clone(),
            "u".to_string(),
            "p".to_string(),
        );
        let mut imp = openrtb::Imp::default();
        imp.id = "imp1".to_string();
        imp.banner = Some(Banner::default());
        imp.ext = Some(serde_json::json!({"bidder": {"accountId": 1}}));
        let req = openrtb::BidRequest {
            id: "r".to_string(),
            imp: vec![imp],
            ..Default::default()
        };
        let (requests, _) = adapter.make_requests(&req, &ExtraRequestInfo::default());
        assert_eq!(requests[0].uri, endpoint);
    }

    // ── OpenX ────────────────────────────────────────────────────────────────

    #[test]
    fn test_openx_make_bids_parses_standard_response() {
        let adapter = crate::adapters::OpenxAdapter::new(
            "https://rtb.openx.net/openrtb/2.3/bidrequest".to_string(),
            "openx".to_string(),
        );

        let mut imp = openrtb::Imp::default();
        imp.id = "imp1".to_string();
        imp.banner = Some(Banner::default());
        imp.ext = Some(
            serde_json::json!({"bidder": {"delDomain": "ox-d.example.com", "unit": "12345"}}),
        );
        let internal_req = openrtb::BidRequest {
            id: "openx-req".to_string(),
            imp: vec![imp],
            ..Default::default()
        };

        let bid_response = serde_json::json!({
            "id": "resp1",
            "cur": "USD",
            "seatbid": [{
                "bid": [{
                    "id": "bid1",
                    "impid": "imp1",
                    "price": 1.50,
                    "adm": "<div>ad</div>",
                    "crid": "creative1"
                }]
            }]
        });
        let body = serde_json::to_vec(&bid_response).unwrap();
        let response = ResponseData::new(200, body);
        let external_req =
            RequestData::new_post("https://rtb.openx.net/openrtb/2.3/bidrequest", vec![]);

        let result = adapter.make_bids(&internal_req, &external_req, &response);
        assert!(result.is_ok(), "make_bids should succeed");
        let bidder_response = result.unwrap();
        assert_eq!(bidder_response.bids.len(), 1);
        assert_eq!(bidder_response.bids[0].bid.id, "bid1");
        assert_eq!(bidder_response.bids[0].bid.price, 1.50);
        assert_eq!(bidder_response.currency, "USD");
    }

    #[test]
    fn test_openx_make_bids_no_content_returns_empty() {
        let adapter = crate::adapters::OpenxAdapter::new(
            "https://rtb.openx.net/openrtb/2.3/bidrequest".to_string(),
            "openx".to_string(),
        );
        let internal_req = openrtb::BidRequest::default();
        let external_req = RequestData::new_post("https://rtb.openx.net", vec![]);
        let response = ResponseData::new(204, vec![]);

        let result = adapter.make_bids(&internal_req, &external_req, &response);
        assert!(result.is_ok());
        assert!(result.unwrap().bids.is_empty());
    }

    // ── Sovrn ────────────────────────────────────────────────────────────────

    #[test]
    fn test_sovrn_make_requests_sets_content_type_header() {
        let adapter =
            crate::adapters::SovrnAdapter::new("https://ap.lijit.com/rtb/bid".to_string());
        let mut imp = openrtb::Imp::default();
        imp.id = "imp1".to_string();
        imp.banner = Some(Banner::default());
        imp.ext = Some(serde_json::json!({"bidder": {"tagid": "696969"}}));
        let req = openrtb::BidRequest {
            id: "sovrn-req".to_string(),
            imp: vec![imp],
            ..Default::default()
        };
        let (requests, errors) = adapter.make_requests(&req, &ExtraRequestInfo::default());
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
        assert_eq!(requests.len(), 1);
        let rd = &requests[0];
        // Sovrn serialises the request as plain JSON (no gzip on the wire).
        // The endpoint_compression equivalent is the Content-Type header.
        let ct = rd
            .headers
            .get("Content-Type")
            .expect("Content-Type must be set");
        assert!(
            ct.contains("application/json"),
            "Content-Type should be application/json, got: {ct}"
        );
        assert_eq!(rd.method, "POST");
    }

    #[test]
    fn test_sovrn_make_requests_device_headers_forwarded() {
        let adapter =
            crate::adapters::SovrnAdapter::new("https://ap.lijit.com/rtb/bid".to_string());
        let mut imp = openrtb::Imp::default();
        imp.id = "imp1".to_string();
        imp.banner = Some(Banner::default());
        imp.ext = Some(serde_json::json!({"bidder": {"tagid": "696969"}}));
        let req = openrtb::BidRequest {
            id: "sovrn-dev".to_string(),
            imp: vec![imp],
            device: Some(openrtb::Device {
                ua: Some("TestAgent/1.0".to_string()),
                ip: Some("1.2.3.4".to_string()),
                language: Some("en".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let (requests, errors) = adapter.make_requests(&req, &ExtraRequestInfo::default());
        assert!(errors.is_empty());
        let rd = &requests[0];
        assert_eq!(
            rd.headers.get("User-Agent").map(String::as_str),
            Some("TestAgent/1.0")
        );
        assert_eq!(
            rd.headers.get("X-Forwarded-For").map(String::as_str),
            Some("1.2.3.4")
        );
        assert_eq!(
            rd.headers.get("Accept-Language").map(String::as_str),
            Some("en")
        );
    }

    // ── 33across ─────────────────────────────────────────────────────────────

    #[test]
    fn test_33across_video_missing_protocols_produces_error() {
        let adapter = crate::adapters::Across33Adapter::new(
            "https://ssc.33across.com/api/v1/hb".to_string(),
        );

        // Video imp without protocols or mimes — should trigger validation error
        let mut video = Video::default();
        video.w = Some(640);
        video.h = Some(480);
        // protocols and mimes are deliberately left None

        let mut imp = openrtb::Imp::default();
        imp.id = "imp1".to_string();
        imp.video = Some(video);
        imp.ext = Some(serde_json::json!({
            "bidder": {
                "productId": "instream",
                "siteId": "site123"
            }
        }));

        let req = openrtb::BidRequest {
            id: "ttx-req".to_string(),
            imp: vec![imp],
            ..Default::default()
        };

        let (requests, errors) = adapter.make_requests(&req, &ExtraRequestInfo::default());
        // The invalid video imp should produce an error and no valid request
        assert!(
            !errors.is_empty(),
            "expected validation error for missing video fields"
        );
        assert!(
            requests.is_empty(),
            "should produce no requests for invalid video imp"
        );
    }

    #[test]
    fn test_33across_banner_make_requests_succeeds() {
        let adapter = crate::adapters::Across33Adapter::new(
            "https://ssc.33across.com/api/v1/hb".to_string(),
        );

        let mut imp = openrtb::Imp::default();
        imp.id = "imp1".to_string();
        imp.banner = Some(Banner {
            format: Some(vec![make_format(300, 250)]),
            ..Default::default()
        });
        imp.ext = Some(serde_json::json!({
            "bidder": {
                "productId": "siab",
                "siteId": "site123"
            }
        }));

        let req = openrtb::BidRequest {
            id: "ttx-req".to_string(),
            imp: vec![imp],
            ..Default::default()
        };

        let (requests, errors) = adapter.make_requests(&req, &ExtraRequestInfo::default());
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].method, "POST");
        assert_eq!(requests[0].uri, "https://ssc.33across.com/api/v1/hb");
    }

    // ── Yandex ───────────────────────────────────────────────────────────────

    #[test]
    fn test_yandex_makes_per_imp_request() {
        // Two imps with different placement IDs → should produce 2 separate requests.
        let adapter = crate::adapters2::yandex::YandexAdapter::new(
            "https://bidding.yandex.net/openrtb/2.5".to_string(),
        );

        let imp1 = openrtb::Imp {
            id: "imp1".to_string(),
            banner: Some(Banner {
                format: Some(vec![make_format(300, 250)]),
                w: Some(300),
                h: Some(250),
                ..Default::default()
            }),
            ext: Some(serde_json::json!({"bidder": {"placementId": "123456-78901"}})),
            ..Default::default()
        };
        let imp2 = openrtb::Imp {
            id: "imp2".to_string(),
            banner: Some(Banner {
                format: Some(vec![make_format(728, 90)]),
                w: Some(728),
                h: Some(90),
                ..Default::default()
            }),
            ext: Some(serde_json::json!({"bidder": {"placementId": "234567-89012"}})),
            ..Default::default()
        };

        let req = openrtb::BidRequest {
            id: "yandex-req".to_string(),
            imp: vec![imp1, imp2],
            ..Default::default()
        };

        let (requests, errors) = adapter.make_requests(&req, &ExtraRequestInfo::default());
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
        assert_eq!(
            requests.len(),
            2,
            "yandex should create one request per imp; got {} requests",
            requests.len()
        );
        for rd in &requests {
            assert_eq!(rd.method, "POST");
            assert!(!rd.body.is_empty(), "request body should not be empty");
        }
    }

    #[test]
    fn test_yandex_invalid_placement_id_produces_error() {
        let adapter = crate::adapters2::yandex::YandexAdapter::new(
            "https://bidding.yandex.net/openrtb/2.5".to_string(),
        );

        let imp = openrtb::Imp {
            id: "imp1".to_string(),
            banner: Some(Banner {
                format: Some(vec![make_format(300, 250)]),
                w: Some(300),
                h: Some(250),
                ..Default::default()
            }),
            // placementId has only one numeric part
            ext: Some(serde_json::json!({"bidder": {"placementId": "only-one"}})),
            ..Default::default()
        };

        let req = openrtb::BidRequest {
            id: "yandex-bad".to_string(),
            imp: vec![imp],
            ..Default::default()
        };

        let (requests, errors) = adapter.make_requests(&req, &ExtraRequestInfo::default());
        assert!(!errors.is_empty(), "should produce an error for invalid placement id");
        assert!(requests.is_empty(), "should produce no requests for invalid placement id");
    }

    // ── Smaato ───────────────────────────────────────────────────────────────

    #[test]
    fn test_smaato_banner_request() {
        // Standard banner imp → should produce exactly one request.
        let adapter = crate::adapters2::smaato::SmaatoAdapter::new(
            "https://prebid.smaato.net/oapi/prebid".to_string(),
        );

        let imp = openrtb::Imp {
            id: "imp1".to_string(),
            banner: Some(Banner {
                format: Some(vec![make_format(300, 250)]),
                ..Default::default()
            }),
            ext: Some(serde_json::json!({
                "bidder": {
                    "publisherId": "pub123",
                    "adspaceId": "ads456"
                }
            })),
            ..Default::default()
        };

        let req = openrtb::BidRequest {
            id: "smaato-req".to_string(),
            imp: vec![imp],
            site: Some(openrtb::Site {
                page: Some("https://example.com".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        let (requests, errors) = adapter.make_requests(&req, &ExtraRequestInfo::default());
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
        assert_eq!(requests.len(), 1, "smaato banner should produce exactly one request");
        let rd = &requests[0];
        assert_eq!(rd.method, "POST");
        assert_eq!(rd.uri, "https://prebid.smaato.net/oapi/prebid");
        assert!(!rd.body.is_empty());
    }

    #[test]
    fn test_smaato_missing_publisher_id_produces_error() {
        let adapter = crate::adapters2::smaato::SmaatoAdapter::new(
            "https://prebid.smaato.net/oapi/prebid".to_string(),
        );

        let imp = openrtb::Imp {
            id: "imp1".to_string(),
            banner: Some(Banner::default()),
            // No publisherId in bidder ext
            ext: Some(serde_json::json!({"bidder": {"adspaceId": "ads456"}})),
            ..Default::default()
        };

        let req = openrtb::BidRequest {
            id: "smaato-no-pub".to_string(),
            imp: vec![imp],
            ..Default::default()
        };

        let (requests, errors) = adapter.make_requests(&req, &ExtraRequestInfo::default());
        assert!(
            !errors.is_empty(),
            "should produce an error when publisherId is missing"
        );
        assert!(
            requests.is_empty(),
            "should produce no requests when publisherId is missing"
        );
    }

    // ── BidderError helpers ──────────────────────────────────────────────────

    #[test]
    fn test_bidder_error_bad_input_message() {
        let e = BidderError::bad_input("something wrong");
        assert!(e.to_string().contains("something wrong"));
        assert!(e.is_fatal());
    }

    #[test]
    fn test_bidder_error_bad_server_response_not_fatal() {
        let e = BidderError::bad_server_response("server exploded");
        assert!(e.to_string().contains("server exploded"));
        assert!(!e.is_fatal());
    }

    #[test]
    fn test_bidder_error_timeout_not_fatal() {
        let e = BidderError::Timeout;
        assert!(!e.is_fatal());
    }

    // ── RequestData helpers ──────────────────────────────────────────────────

    #[test]
    fn test_request_data_new_sets_method_and_uri() {
        let rd = RequestData::new("GET", "http://example.com/test", vec![]);
        assert_eq!(rd.method, "GET");
        assert_eq!(rd.uri, "http://example.com/test");
        assert!(rd.body.is_empty());
        assert!(rd.imp_ids.is_empty());
    }

    #[test]
    fn test_request_data_set_header_overwrites() {
        let mut rd = RequestData::new_post("http://example.com", vec![]);
        rd.set_header("Content-Type", "text/plain");
        assert_eq!(
            rd.headers.get("Content-Type").map(String::as_str),
            Some("text/plain")
        );
    }

    // ── get_bid_type_from_mtype edge cases ───────────────────────────────────

    #[test]
    fn test_get_bid_type_from_mtype_all_values() {
        use openrtb_ext::BidType;
        assert_eq!(get_bid_type_from_mtype(1), BidType::Banner);
        assert_eq!(get_bid_type_from_mtype(2), BidType::Video);
        assert_eq!(get_bid_type_from_mtype(3), BidType::Audio);
        assert_eq!(get_bid_type_from_mtype(4), BidType::Native);
        // Unknown mtype defaults to Banner
        assert_eq!(get_bid_type_from_mtype(5), BidType::Banner);
        assert_eq!(get_bid_type_from_mtype(-1), BidType::Banner);
        assert_eq!(get_bid_type_from_mtype(100), BidType::Banner);
    }

    #[test]
    fn test_check_response_status_all_cases() {
        assert!(check_response_status(200).is_ok());
        assert!(check_response_status(204).is_err());
        assert!(check_response_status(400).is_err());
        assert!(check_response_status(404).is_err());
        assert!(check_response_status(500).is_err());
        assert!(check_response_status(503).is_err());
    }
}
