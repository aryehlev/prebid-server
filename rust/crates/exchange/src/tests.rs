use super::*;
use openrtb::{BidRequest, Imp, Banner, Format, Video};
use std::collections::HashMap;
use std::sync::Arc;

// Mock bidder that returns a fixed bid
struct MockBidder {
    price: f64,
    bid_type: openrtb_ext::BidType,
}

impl pbs_adapters::Bidder for MockBidder {
    fn make_requests(
        &self,
        request: &BidRequest,
        _: &pbs_adapters::ExtraRequestInfo,
    ) -> (Vec<pbs_adapters::RequestData>, Vec<pbs_adapters::BidderError>) {
        let body = serde_json::to_vec(request).unwrap();
        (
            vec![pbs_adapters::RequestData::new_post("http://mock", body)],
            vec![],
        )
    }

    fn make_bids(
        &self,
        _: &BidRequest,
        _: &pbs_adapters::RequestData,
        _: &pbs_adapters::ResponseData,
    ) -> Result<pbs_adapters::BidderResponse, Vec<pbs_adapters::BidderError>> {
        let bid = openrtb::Bid {
            id: "bid1".to_string(),
            impid: "imp1".to_string(),
            price: self.price,
            crid: Some("cr1".to_string()),
            ..Default::default()
        };
        let mut resp = pbs_adapters::BidderResponse::new();
        resp.bids
            .push(pbs_adapters::TypedBid::new(bid, self.bid_type.clone()));
        Ok(resp)
    }
}

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

fn make_simple_request() -> BidRequest {
    BidRequest {
        id: "test-req".to_string(),
        imp: vec![Imp {
            id: "imp1".to_string(),
            banner: Some(Banner {
                format: Some(vec![make_format(300, 250)]),
                ..Default::default()
            }),
            ext: Some(serde_json::json!({
                "appnexus": {}
            })),
            ..Default::default()
        }],
        ..Default::default()
    }
}

#[allow(dead_code)]
fn make_adapted_bidder(price: f64) -> AdaptedBidder {
    AdaptedBidder {
        bidder: Arc::new(MockBidder {
            price,
            bid_type: openrtb_ext::BidType::Banner,
        }),
        http_client: reqwest::Client::new(),
        endpoint: "http://mock".to_string(),
        endpoint_compression: None,
    }
}

#[tokio::test]
async fn test_hold_auction_empty_request_fails() {
    let exchange = Exchange::new(HashMap::new());
    let req = AuctionRequest {
        bid_request: BidRequest {
            id: "x".to_string(),
            imp: vec![],
            ..Default::default()
        },
        account: None,
        user_syncs: None,
        start_time: std::time::Instant::now(),
        currency_rates: None,
    };
    assert!(exchange.hold_auction(req).await.is_err());
}

#[tokio::test]
async fn test_hold_auction_no_matching_bidders() {
    // Exchange has no registered adapters, so no bidder matches the imp ext
    let exchange = Exchange::new(HashMap::new());
    let req = AuctionRequest {
        bid_request: make_simple_request(),
        account: None,
        user_syncs: None,
        start_time: std::time::Instant::now(),
        currency_rates: None,
    };
    let result = exchange.hold_auction(req).await.unwrap();
    assert!(result.bid_response.seatbid.is_empty());
}

#[tokio::test]
async fn test_hold_auction_imp_without_media_type_fails() {
    let exchange = Exchange::new(HashMap::new());
    let req = AuctionRequest {
        bid_request: BidRequest {
            id: "x".to_string(),
            imp: vec![Imp {
                id: "imp1".to_string(),
                // No banner/video/audio/native
                ..Default::default()
            }],
            ..Default::default()
        },
        account: None,
        user_syncs: None,
        start_time: std::time::Instant::now(),
        currency_rates: None,
    };
    let result = exchange.hold_auction(req).await;
    assert!(result.is_err());
    let err = result.err().unwrap().to_string();
    assert!(
        err.contains("banner/video/audio/native"),
        "unexpected error: {err}"
    );
}

#[tokio::test]
async fn test_hold_auction_imp_without_id_fails() {
    let exchange = Exchange::new(HashMap::new());
    let req = AuctionRequest {
        bid_request: BidRequest {
            id: "x".to_string(),
            imp: vec![Imp {
                id: "".to_string(),
                banner: Some(Banner::default()),
                ..Default::default()
            }],
            ..Default::default()
        },
        account: None,
        user_syncs: None,
        start_time: std::time::Instant::now(),
        currency_rates: None,
    };
    let result = exchange.hold_auction(req).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_price_floor_filtering() {
    // No bidder registered for "appnexus" — result should be empty seatbids.
    // This exercises the request validation path that leads to floor filtering.
    let exchange = Exchange::new(HashMap::new());
    let mut req = make_simple_request();
    req.imp[0].bidfloor = Some(1.0);
    let auction_req = AuctionRequest {
        bid_request: req,
        account: None,
        user_syncs: None,
        start_time: std::time::Instant::now(),
        currency_rates: None,
    };
    let result = exchange.hold_auction(auction_req).await.unwrap();
    assert!(result.bid_response.seatbid.is_empty());
}

#[tokio::test]
async fn test_hold_auction_negative_tmax_fails() {
    let exchange = Exchange::new(HashMap::new());
    let req = AuctionRequest {
        bid_request: BidRequest {
            id: "x".to_string(),
            tmax: Some(-1),
            imp: vec![Imp {
                id: "imp1".to_string(),
                banner: Some(Banner::default()),
                ..Default::default()
            }],
            ..Default::default()
        },
        account: None,
        user_syncs: None,
        start_time: std::time::Instant::now(),
        currency_rates: None,
    };
    let result = exchange.hold_auction(req).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_response_has_correct_id() {
    let exchange = Exchange::new(HashMap::new());
    let req = AuctionRequest {
        bid_request: make_simple_request(),
        account: None,
        user_syncs: None,
        start_time: std::time::Instant::now(),
        currency_rates: None,
    };
    let result = exchange.hold_auction(req).await.unwrap();
    assert_eq!(result.bid_response.id, "test-req");
}

#[tokio::test]
async fn test_response_currency_is_usd() {
    let exchange = Exchange::new(HashMap::new());
    let req = AuctionRequest {
        bid_request: make_simple_request(),
        account: None,
        user_syncs: None,
        start_time: std::time::Instant::now(),
        currency_rates: None,
    };
    let result = exchange.hold_auction(req).await.unwrap();
    assert_eq!(result.bid_response.cur.as_deref(), Some("USD"));
}

#[tokio::test]
async fn test_hold_auction_video_imp() {
    let exchange = Exchange::new(HashMap::new());
    let req = AuctionRequest {
        bid_request: BidRequest {
            id: "video-req".to_string(),
            imp: vec![Imp {
                id: "imp1".to_string(),
                video: Some(Video::default()),
                ext: Some(serde_json::json!({ "appnexus": {} })),
                ..Default::default()
            }],
            ..Default::default()
        },
        account: None,
        user_syncs: None,
        start_time: std::time::Instant::now(),
        currency_rates: None,
    };
    // No registered bidder for appnexus, so auction succeeds with empty seatbids
    let result = exchange.hold_auction(req).await.unwrap();
    assert!(result.bid_response.seatbid.is_empty());
}

/// A bidder that hits the given URI and parses a standard OpenRTB BidResponse.
struct HttpMockBidder {
    uri: String,
}

impl pbs_adapters::Bidder for HttpMockBidder {
    fn make_requests(
        &self,
        request: &BidRequest,
        _: &pbs_adapters::ExtraRequestInfo,
    ) -> (Vec<pbs_adapters::RequestData>, Vec<pbs_adapters::BidderError>) {
        let body = serde_json::to_vec(request).unwrap();
        (
            vec![pbs_adapters::RequestData::new_post(&self.uri, body)],
            vec![],
        )
    }

    fn make_bids(
        &self,
        request: &BidRequest,
        _: &pbs_adapters::RequestData,
        response: &pbs_adapters::ResponseData,
    ) -> Result<pbs_adapters::BidderResponse, Vec<pbs_adapters::BidderError>> {
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![pbs_adapters::BidderError::BadServerResponse(e.to_string())])?;

        let mut result = pbs_adapters::BidderResponse::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = request
                    .imp
                    .iter()
                    .find(|i| i.id == bid.impid)
                    .map(|imp| {
                        if imp.video.is_some() {
                            openrtb_ext::BidType::Video
                        } else {
                            openrtb_ext::BidType::Banner
                        }
                    })
                    .unwrap_or(openrtb_ext::BidType::Banner);
                result.bids.push(pbs_adapters::TypedBid::new(bid, bid_type));
            }
        }
        Ok(result)
    }
}

#[tokio::test]
async fn test_bid_adjustment_factor_applied() {
    use wiremock::{MockServer, Mock, ResponseTemplate};
    use wiremock::matchers::method;

    // Spin up a mock HTTP server that returns a bid with price 1.0.
    let mock_server = MockServer::start().await;
    let bid_response_json = serde_json::json!({
        "id": "resp1",
        "seatbid": [{
            "bid": [{
                "id": "bid1",
                "impid": "imp1",
                "price": 1.0,
                "adm": "<ad/>", "crid": "cr1"
            }]
        }]
    });
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&bid_response_json))
        .mount(&mock_server)
        .await;

    let uri = mock_server.uri();
    let mut adapters = HashMap::new();
    adapters.insert(
        "appnexus".to_string(),
        AdaptedBidder {
            bidder: Arc::new(HttpMockBidder { uri }),
            http_client: reqwest::Client::new(),
            endpoint: mock_server.uri(),
            endpoint_compression: None,
        },
    );

    let exchange = Exchange::new(adapters);

    let mut req = make_simple_request();
    // Apply a 0.9 factor for appnexus — bid price 1.0 should become 0.9.
    req.ext = Some(serde_json::json!({
        "prebid": {
            "bidadjustmentfactors": {
                "appnexus": 0.9,
                "rubicon": 1.1
            }
        }
    }));

    let result = exchange
        .hold_auction(AuctionRequest {
            bid_request: req,
            account: None,
            user_syncs: None,
            start_time: std::time::Instant::now(),
            currency_rates: None,
        })
        .await
        .unwrap();

    assert_eq!(result.bid_response.seatbid.len(), 1);
    let seat = &result.bid_response.seatbid[0];
    assert_eq!(seat.seat.as_deref(), Some("appnexus"));
    assert_eq!(seat.bid.len(), 1);

    let adjusted_price = seat.bid[0].price;
    assert!(
        (adjusted_price - 0.9).abs() < 1e-9,
        "expected adjusted price 0.9, got {adjusted_price}"
    );
}

#[tokio::test]
async fn test_bid_no_adjustment_factor_unchanged() {
    use wiremock::{MockServer, Mock, ResponseTemplate};
    use wiremock::matchers::method;

    // Spin up a mock HTTP server that returns a bid with price 2.0.
    let mock_server = MockServer::start().await;
    let bid_response_json = serde_json::json!({
        "id": "resp1",
        "seatbid": [{
            "bid": [{
                "id": "bid1",
                "impid": "imp1",
                "price": 2.0,
                "adm": "<ad/>", "crid": "cr1"
            }]
        }]
    });
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&bid_response_json))
        .mount(&mock_server)
        .await;

    let uri = mock_server.uri();
    let mut adapters = HashMap::new();
    adapters.insert(
        "appnexus".to_string(),
        AdaptedBidder {
            bidder: Arc::new(HttpMockBidder { uri }),
            http_client: reqwest::Client::new(),
            endpoint: mock_server.uri(),
            endpoint_compression: None,
        },
    );

    let exchange = Exchange::new(adapters);

    let mut req = make_simple_request();
    // Provide a factor only for an unrelated bidder — appnexus bid should be unchanged at 2.0.
    req.ext = Some(serde_json::json!({
        "prebid": {
            "bidadjustmentfactors": {
                "rubicon": 1.5
            }
        }
    }));

    let result = exchange
        .hold_auction(AuctionRequest {
            bid_request: req,
            account: None,
            user_syncs: None,
            start_time: std::time::Instant::now(),
            currency_rates: None,
        })
        .await
        .unwrap();

    assert_eq!(result.bid_response.seatbid.len(), 1);
    let seat = &result.bid_response.seatbid[0];
    assert_eq!(seat.bid.len(), 1);

    let price = seat.bid[0].price;
    assert!(
        (price - 2.0).abs() < 1e-9,
        "expected unchanged price 2.0, got {price}"
    );
}

#[test]
fn test_auction_request_fields() {
    let req = AuctionRequest {
        bid_request: BidRequest {
            id: "test".to_string(),
            ..Default::default()
        },
        account: Some(Account {
            id: "acct1".to_string(),
            price_granularity: Some("medium".to_string()),
        }),
        user_syncs: Some(UserSyncData {
            synced_bidders: std::collections::HashSet::new(),
        }),
        start_time: std::time::Instant::now(),
        currency_rates: None,
    };
    assert_eq!(req.bid_request.id, "test");
    assert_eq!(req.account.as_ref().unwrap().id, "acct1");
}

// ── Helper: spin up a wiremock server returning a single bid at the given price/nurl ──────────

async fn make_mock_server_with_bid(
    price: f64,
    adm: Option<&str>,
    nurl: Option<&str>,
    currency: Option<&str>,
    delay_ms: Option<u64>,
) -> wiremock::MockServer {
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wiremock::matchers::method;

    let mock_server = MockServer::start().await;
    let mut bid = serde_json::json!({
        "id": "bid1",
        "impid": "imp1",
        "price": price,
        "crid": "cr1"
    });
    if let Some(a) = adm {
        bid["adm"] = serde_json::Value::String(a.to_string());
    }
    if let Some(n) = nurl {
        bid["nurl"] = serde_json::Value::String(n.to_string());
    }
    let mut resp_body = serde_json::json!({
        "id": "resp1",
        "seatbid": [{"bid": [bid]}]
    });
    if let Some(cur) = currency {
        resp_body["cur"] = serde_json::Value::String(cur.to_string());
    }
    let mut template = ResponseTemplate::new(200).set_body_json(&resp_body);
    if let Some(d) = delay_ms {
        template = template.set_delay(std::time::Duration::from_millis(d));
    }
    Mock::given(method("POST"))
        .respond_with(template)
        .mount(&mock_server)
        .await;
    mock_server
}

fn make_exchange_with_bidder(uri: String) -> Exchange {
    let mut adapters = HashMap::new();
    adapters.insert(
        "appnexus".to_string(),
        AdaptedBidder {
            bidder: Arc::new(HttpMockBidder { uri }),
            http_client: reqwest::Client::new(),
            endpoint: String::new(),
            endpoint_compression: None,
        },
    );
    Exchange::new(adapters)
}

// ── A mock bidder that returns a BidderResponse with the given currency field ─────────────────

struct CurrencyMockBidder {
    price: f64,
    currency: String,
}

impl pbs_adapters::Bidder for CurrencyMockBidder {
    fn make_requests(
        &self,
        request: &BidRequest,
        _: &pbs_adapters::ExtraRequestInfo,
    ) -> (Vec<pbs_adapters::RequestData>, Vec<pbs_adapters::BidderError>) {
        let body = serde_json::to_vec(request).unwrap();
        (vec![pbs_adapters::RequestData::new_post("http://mock", body)], vec![])
    }

    fn make_bids(
        &self,
        _: &BidRequest,
        _: &pbs_adapters::RequestData,
        _: &pbs_adapters::ResponseData,
    ) -> Result<pbs_adapters::BidderResponse, Vec<pbs_adapters::BidderError>> {
        let bid = openrtb::Bid {
            id: "bid1".to_string(),
            impid: "imp1".to_string(),
            price: self.price,
            adm: Some("<ad/>".to_string()),
            crid: Some("cr1".to_string()),
            ..Default::default()
        };
        let mut resp = pbs_adapters::BidderResponse::new();
        resp.currency = self.currency.clone();
        resp.bids.push(pbs_adapters::TypedBid::new(bid, openrtb_ext::BidType::Banner));
        Ok(resp)
    }
}

// ── A mock bidder that returns two bids with the same id at different prices ──────────────────

struct DupBidMockBidder;

impl pbs_adapters::Bidder for DupBidMockBidder {
    fn make_requests(
        &self,
        request: &BidRequest,
        _: &pbs_adapters::ExtraRequestInfo,
    ) -> (Vec<pbs_adapters::RequestData>, Vec<pbs_adapters::BidderError>) {
        let body = serde_json::to_vec(request).unwrap();
        (vec![pbs_adapters::RequestData::new_post("http://mock", body)], vec![])
    }

    fn make_bids(
        &self,
        _: &BidRequest,
        _: &pbs_adapters::RequestData,
        _: &pbs_adapters::ResponseData,
    ) -> Result<pbs_adapters::BidderResponse, Vec<pbs_adapters::BidderError>> {
        let low = openrtb::Bid {
            id: "dup-bid".to_string(),
            impid: "imp1".to_string(),
            price: 1.0,
            adm: Some("<low/>".to_string()),
            crid: Some("cr1".to_string()),
            ..Default::default()
        };
        let high = openrtb::Bid {
            id: "dup-bid".to_string(),
            impid: "imp1".to_string(),
            price: 3.0,
            adm: Some("<high/>".to_string()),
            crid: Some("cr1".to_string()),
            ..Default::default()
        };
        let mut resp = pbs_adapters::BidderResponse::new();
        resp.bids.push(pbs_adapters::TypedBid::new(low, openrtb_ext::BidType::Banner));
        resp.bids.push(pbs_adapters::TypedBid::new(high, openrtb_ext::BidType::Banner));
        Ok(resp)
    }
}

// ── Test 1: bid adjustment factor 0.5 applied to price 2.0 → final price 1.0 ─────────────────

#[tokio::test]
async fn test_bid_adjustment_factor_half() {
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wiremock::matchers::method;

    let mock_server = MockServer::start().await;
    let body = serde_json::json!({"id":"r","seatbid":[{"bid":[{"id":"b1","impid":"imp1","price":2.0,"adm":"<ad/>","crid":"cr1"}]}]});
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&body))
        .mount(&mock_server)
        .await;

    let exchange = make_exchange_with_bidder(mock_server.uri());
    let mut req = make_simple_request();
    req.ext = Some(serde_json::json!({"prebid":{"bidadjustmentfactors":{"appnexus":0.5}}}));

    let result = exchange.hold_auction(AuctionRequest {
        bid_request: req, account: None, user_syncs: None,
        start_time: std::time::Instant::now(), currency_rates: None,
    }).await.unwrap();

    let price = result.bid_response.seatbid[0].bid[0].price;
    assert!((price - 1.0).abs() < 1e-9, "expected 1.0, got {price}");
}

// ── Test 2: bid validation — empty ID is dropped ──────────────────────────────────────────────

#[test]
fn test_validate_bids_empty_id_dropped() {
    let imp = openrtb::Imp { id: "imp1".to_string(), ..Default::default() };
    let bad = pbs_adapters::TypedBid::new(
        openrtb::Bid { id: "".to_string(), impid: "imp1".to_string(), price: 1.0, ..Default::default() },
        openrtb_ext::BidType::Banner,
    );
    let kept = validate_bids(vec![bad], &[imp]);
    assert!(kept.is_empty(), "bid with empty id should be dropped");
}

// ── Test 3: bid validation — wrong impid is dropped ───────────────────────────────────────────

#[test]
fn test_validate_bids_wrong_impid_dropped() {
    let imp = openrtb::Imp { id: "imp1".to_string(), ..Default::default() };
    let bad = pbs_adapters::TypedBid::new(
        openrtb::Bid { id: "b1".to_string(), impid: "wrong-imp".to_string(), price: 1.0, crid: Some("cr1".to_string()), ..Default::default() },
        openrtb_ext::BidType::Banner,
    );
    let kept = validate_bids(vec![bad], &[imp]);
    assert!(kept.is_empty(), "bid with wrong impid should be dropped");
}

// ── Test 4: bid validation — negative price is dropped ───────────────────────────────────────

#[test]
fn test_validate_bids_negative_price_dropped() {
    let imp = openrtb::Imp { id: "imp1".to_string(), ..Default::default() };
    let bad = pbs_adapters::TypedBid::new(
        openrtb::Bid { id: "b1".to_string(), impid: "imp1".to_string(), price: -0.01, crid: Some("cr1".to_string()), ..Default::default() },
        openrtb_ext::BidType::Banner,
    );
    let kept = validate_bids(vec![bad], &[imp]);
    assert!(kept.is_empty(), "bid with negative price should be dropped");
}

// ── Test: bid validation — zero price without deal is dropped ────────────────────────────────

#[test]
fn test_validate_bids_zero_price_no_deal_dropped() {
    let imp = openrtb::Imp { id: "imp1".to_string(), ..Default::default() };
    let bad = pbs_adapters::TypedBid::new(
        openrtb::Bid { id: "b1".to_string(), impid: "imp1".to_string(), price: 0.0, crid: Some("cr1".to_string()), ..Default::default() },
        openrtb_ext::BidType::Banner,
    );
    let kept = validate_bids(vec![bad], &[imp]);
    assert!(kept.is_empty(), "bid with zero price and no deal should be dropped");
}

// ── Test: bid validation — zero price WITH deal is kept ──────────────────────────────────────

#[test]
fn test_validate_bids_zero_price_with_deal_kept() {
    let imp = openrtb::Imp { id: "imp1".to_string(), ..Default::default() };
    let good = pbs_adapters::TypedBid::new(
        openrtb::Bid {
            id: "b1".to_string(), impid: "imp1".to_string(), price: 0.0,
            crid: Some("cr1".to_string()), dealid: Some("deal-123".to_string()),
            ..Default::default()
        },
        openrtb_ext::BidType::Banner,
    );
    let kept = validate_bids(vec![good], &[imp]);
    assert_eq!(kept.len(), 1, "bid with zero price and deal should be kept");
}

// ── Test: bid validation — missing creative ID is dropped ────────────────────────────────────

#[test]
fn test_validate_bids_missing_crid_dropped() {
    let imp = openrtb::Imp { id: "imp1".to_string(), ..Default::default() };
    let bad = pbs_adapters::TypedBid::new(
        openrtb::Bid { id: "b1".to_string(), impid: "imp1".to_string(), price: 1.0, ..Default::default() },
        openrtb_ext::BidType::Banner,
    );
    let kept = validate_bids(vec![bad], &[imp]);
    assert!(kept.is_empty(), "bid with empty crid should be dropped");
}

// ── Test: currency validation ────────────────────────────────────────────────────────────────

#[test]
fn test_validate_bid_currency_default_usd() {
    assert!(validate_bid_currency(&[], "").is_ok());
    assert!(validate_bid_currency(&[], "USD").is_ok());
}

#[test]
fn test_validate_bid_currency_valid_codes() {
    assert!(validate_bid_currency(&[], "EUR").is_ok());
    assert!(validate_bid_currency(&[], "GBP").is_ok());
    assert!(validate_bid_currency(&[], "JPY").is_ok());
}

#[test]
fn test_validate_bid_currency_invalid_code() {
    assert!(validate_bid_currency(&[], "XY").is_err());
    assert!(validate_bid_currency(&[], "1234").is_err());
    assert!(validate_bid_currency(&[], "ab").is_err());
}

// ── Test 5: GDPR with no consent → seat_non_bid with status code 50 ──────────────────────────

#[tokio::test]
async fn test_gdpr_no_consent_produces_seat_non_bid_50() {
    let mut adapters = HashMap::new();
    adapters.insert("appnexus".to_string(), AdaptedBidder {
        bidder: Arc::new(HttpMockBidder { uri: "http://unused".to_string() }),
        http_client: reqwest::Client::new(),
        endpoint: String::new(),
        endpoint_compression: None,
    });
    let exchange = Exchange::new(adapters);

    let mut req = make_simple_request();
    req.regs = Some(openrtb::Regs { ext: Some(serde_json::json!({"gdpr": 1})), ..Default::default() });
    // No user.ext.consent → GDPR blocks the bidder.

    let result = exchange.hold_auction(AuctionRequest {
        bid_request: req, account: None, user_syncs: None,
        start_time: std::time::Instant::now(), currency_rates: None,
    }).await.unwrap();

    assert!(result.bid_response.seatbid.is_empty());
    assert_eq!(result.seat_non_bids.len(), 1);
    assert_eq!(result.seat_non_bids[0].seat, "appnexus");
    assert_eq!(result.seat_non_bids[0].nonbid[0].statuscode, 50);
}

// ── Test 6: bid below floor is dropped and produces seat_non_bid with code 300 ───────────────

#[tokio::test]
async fn test_price_floor_drops_bid_and_emits_non_bid_300() {
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wiremock::matchers::method;

    let mock_server = MockServer::start().await;
    // Bid price 0.5 is below floor 1.0.
    let body = serde_json::json!({"id":"r","seatbid":[{"bid":[{"id":"b1","impid":"imp1","price":0.5,"adm":"<ad/>","crid":"cr1"}]}]});
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&body))
        .mount(&mock_server)
        .await;

    let exchange = make_exchange_with_bidder(mock_server.uri());
    let mut req = make_simple_request();
    req.imp[0].bidfloor = Some(1.0);

    let result = exchange.hold_auction(AuctionRequest {
        bid_request: req, account: None, user_syncs: None,
        start_time: std::time::Instant::now(), currency_rates: None,
    }).await.unwrap();

    assert!(result.bid_response.seatbid.is_empty(), "below-floor bid should be dropped");
    assert_eq!(result.seat_non_bids.len(), 1);
    assert_eq!(result.seat_non_bids[0].nonbid[0].statuscode, 300);
}

// ── Test 7: custom price granularity bucket string for $1.23 ──────────────────────────────────

#[test]
fn test_custom_price_granularity_bucket() {
    let gran = PriceGranularity {
        precision: Some(2),
        ranges: vec![
            PriceRange { max: 5.0, increment: 0.5 },
            PriceRange { max: 10.0, increment: 1.0 },
        ],
    };
    // $1.23 falls in the [0, 5] range with $0.50 increments.
    // floor((1.23 - 0) / 0.5) = 2, bucket = 2 * 0.5 + 0 = 1.00
    let bucket = price_granularity_bucket(1.23, Some(&gran));
    assert_eq!(bucket, "1.00", "expected bucket 1.00 for price 1.23, got {bucket}");
}

// ── Test 8: ${AUCTION_PRICE} macro in nurl is replaced with cleared price ─────────────────────

#[tokio::test]
async fn test_macro_resolution_auction_price_in_nurl() {
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wiremock::matchers::method;

    let mock_server = MockServer::start().await;
    let body = serde_json::json!({"id":"r","seatbid":[{"bid":[{"id":"b1","impid":"imp1","price":1.5,"nurl":"http://win?price=${AUCTION_PRICE}","crid":"cr1"}]}]});
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&body))
        .mount(&mock_server)
        .await;

    let exchange = make_exchange_with_bidder(mock_server.uri());

    let result = exchange.hold_auction(AuctionRequest {
        bid_request: make_simple_request(), account: None, user_syncs: None,
        start_time: std::time::Instant::now(), currency_rates: None,
    }).await.unwrap();

    let nurl = result.bid_response.seatbid[0].bid[0].nurl.as_deref().unwrap_or("");
    assert_eq!(nurl, "http://win?price=1.5000", "nurl macro not resolved: {nurl}");
}

// ── Test 9: deduplication — two bids with same id → only higher price kept ───────────────────

#[tokio::test]
async fn test_deduplication_keeps_higher_price() {
    let mut adapters = HashMap::new();
    adapters.insert("appnexus".to_string(), AdaptedBidder {
        bidder: Arc::new(DupBidMockBidder),
        http_client: reqwest::Client::new(),
        endpoint: "http://mock".to_string(),
        endpoint_compression: None,
    });
    let exchange = Exchange::new(adapters);

    let result = exchange.hold_auction(AuctionRequest {
        bid_request: make_simple_request(), account: None, user_syncs: None,
        start_time: std::time::Instant::now(), currency_rates: None,
    }).await.unwrap();

    let bids = &result.bid_response.seatbid[0].bid;
    assert_eq!(bids.len(), 1, "deduplication should keep only one bid");
    assert!((bids[0].price - 3.0).abs() < 1e-9, "expected higher price 3.0, got {}", bids[0].price);
}

// ── Test 10: currency conversion — EUR bid is converted to USD ────────────────────────────────

#[tokio::test]
async fn test_currency_conversion_eur_to_usd() {
    let mut adapters = HashMap::new();
    adapters.insert("appnexus".to_string(), AdaptedBidder {
        bidder: Arc::new(CurrencyMockBidder { price: 1.0, currency: "EUR".to_string() }),
        http_client: reqwest::Client::new(),
        endpoint: "http://mock".to_string(),
        endpoint_compression: None,
    });
    let exchange = Exchange::new(adapters);

    // 1 EUR = 1.10 USD
    let mut rates = HashMap::new();
    let mut eur_rates = HashMap::new();
    eur_rates.insert("USD".to_string(), 1.10_f64);
    rates.insert("EUR".to_string(), eur_rates);
    let converter = Arc::new(currency::CurrencyConverter::new(rates));

    let result = exchange.hold_auction(AuctionRequest {
        bid_request: make_simple_request(), account: None, user_syncs: None,
        start_time: std::time::Instant::now(), currency_rates: Some(converter),
    }).await.unwrap();

    let price = result.bid_response.seatbid[0].bid[0].price;
    assert!((price - 1.10).abs() < 1e-9, "expected 1.10 USD, got {price}");
}

// ── Test 11: targeting keys hb_pb, hb_bidder, hb_adid are set on winning bid ─────────────────

#[tokio::test]
async fn test_targeting_keys_set_on_winning_bid() {
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wiremock::matchers::method;

    let mock_server = MockServer::start().await;
    let body = serde_json::json!({"id":"r","seatbid":[{"bid":[{"id":"bid1","impid":"imp1","price":1.5,"adm":"<ad/>","crid":"cr1"}]}]});
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&body))
        .mount(&mock_server)
        .await;

    let exchange = make_exchange_with_bidder(mock_server.uri());

    let result = exchange.hold_auction(AuctionRequest {
        bid_request: make_simple_request(), account: None, user_syncs: None,
        start_time: std::time::Instant::now(), currency_rates: None,
    }).await.unwrap();

    let keys = result.targeting.get("imp1").expect("targeting for imp1 missing");
    assert!(keys.contains_key("hb_pb"), "hb_pb missing");
    assert_eq!(keys.get("hb_bidder").map(String::as_str), Some("appnexus"));
    assert_eq!(keys.get("hb_adid").map(String::as_str), Some("bid1"));
}

// ── Test 12: tmax timeout causes timed_out=true in response ──────────────────────────────────

#[tokio::test]
async fn test_tmax_timeout_produces_timed_out_bidder() {
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wiremock::matchers::method;

    // The mock server adds a 200ms delay; tmax=1ms forces a timeout.
    let mock_server = MockServer::start().await;
    let body = serde_json::json!({"id":"r","seatbid":[{"bid":[{"id":"b1","impid":"imp1","price":1.0,"adm":"<ad/>","crid":"cr1"}]}]});
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&body).set_delay(std::time::Duration::from_millis(200)))
        .mount(&mock_server)
        .await;

    let exchange = make_exchange_with_bidder(mock_server.uri());
    let mut req = make_simple_request();
    req.tmax = Some(1); // 1ms → bidder will time out

    let result = exchange.hold_auction(AuctionRequest {
        bid_request: req, account: None, user_syncs: None,
        start_time: std::time::Instant::now(), currency_rates: None,
    }).await.unwrap();

    assert!(result.bid_response.seatbid.is_empty(), "timed-out bidder should produce no bids");
    assert!(
        result.timed_out_bidders.contains(&"appnexus".to_string()),
        "appnexus should be in timed_out_bidders"
    );
}

// ── Test 13: debug mode — test=1 populates response.ext.debug.httpcalls ──────────────────────

#[tokio::test]
async fn test_debug_mode_includes_http_calls() {
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wiremock::matchers::method;

    let mock_server = MockServer::start().await;
    let body = serde_json::json!({"id":"r","seatbid":[{"bid":[{"id":"b1","impid":"imp1","price":1.0,"adm":"<ad/>","crid":"cr1"}]}]});
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&body))
        .mount(&mock_server)
        .await;

    let exchange = make_exchange_with_bidder(mock_server.uri());

    // Set test=1 to trigger debug mode.
    let mut req = make_simple_request();
    req.test = Some(1);

    let result = exchange.hold_auction(AuctionRequest {
        bid_request: req, account: None, user_syncs: None,
        start_time: std::time::Instant::now(), currency_rates: None,
    }).await.unwrap();

    // response.ext.debug.httpcalls should be populated
    let debug = result.bid_response.ext
        .as_ref()
        .and_then(|e| e.get("debug"))
        .expect("response.ext.debug should be present when test=1");

    let httpcalls = debug.get("httpcalls")
        .expect("debug.httpcalls should be present");

    // Should have an "appnexus" entry with at least one call
    let appnexus_calls = httpcalls.get("appnexus")
        .expect("debug.httpcalls.appnexus should be present");
    assert!(
        appnexus_calls.as_array().map(|a| !a.is_empty()).unwrap_or(false),
        "httpcalls.appnexus should contain at least one call"
    );
}

// ── Test 14: debug mode — test=0 does NOT populate response.ext.debug ────────────────────────

#[tokio::test]
async fn test_no_debug_mode_when_test_is_zero() {
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wiremock::matchers::method;

    let mock_server = MockServer::start().await;
    let body = serde_json::json!({"id":"r","seatbid":[{"bid":[{"id":"b1","impid":"imp1","price":1.0,"adm":"<ad/>","crid":"cr1"}]}]});
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&body))
        .mount(&mock_server)
        .await;

    let exchange = make_exchange_with_bidder(mock_server.uri());

    // test field absent (default) — no debug block expected
    let result = exchange.hold_auction(AuctionRequest {
        bid_request: make_simple_request(), account: None, user_syncs: None,
        start_time: std::time::Instant::now(), currency_rates: None,
    }).await.unwrap();

    let has_debug = result.bid_response.ext
        .as_ref()
        .and_then(|e| e.get("debug"))
        .is_some();
    assert!(!has_debug, "debug block should NOT be present when test != 1");
}

// ── Test 15: CCPA opt-out — us_privacy "1YYY" skips bidder with SeatNonBid code 51 ────────────

#[tokio::test]
async fn test_ccpa_enforcement_blocks_bidder() {
    let mut adapters = HashMap::new();
    adapters.insert("appnexus".to_string(), AdaptedBidder {
        bidder: Arc::new(HttpMockBidder { uri: "http://unused".to_string() }),
        http_client: reqwest::Client::new(),
        endpoint: String::new(),
        endpoint_compression: None,
    });
    let exchange = Exchange::new(adapters);

    let mut req = make_simple_request();
    // "1YYY" → index 2 == 'Y' → opted out of sale
    req.regs = Some(openrtb::Regs {
        us_privacy: Some("1YYY".to_string()),
        ..Default::default()
    });

    let result = exchange.hold_auction(AuctionRequest {
        bid_request: req, account: None, user_syncs: None,
        start_time: std::time::Instant::now(), currency_rates: None,
    }).await.unwrap();

    assert!(result.bid_response.seatbid.is_empty(), "CCPA opt-out should block all bids");
    assert_eq!(result.seat_non_bids.len(), 1);
    assert_eq!(result.seat_non_bids[0].seat, "appnexus");
    assert_eq!(result.seat_non_bids[0].nonbid[0].statuscode, 51,
        "CCPA opt-out should produce status code 51");
}

// ── Test 16: CCPA no opt-out — us_privacy "1NNN" does NOT block bidder ──────────────────────

#[tokio::test]
async fn test_ccpa_no_opt_out_does_not_block() {
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wiremock::matchers::method;

    let mock_server = MockServer::start().await;
    let body = serde_json::json!({"id":"r","seatbid":[{"bid":[{"id":"b1","impid":"imp1","price":1.0,"adm":"<ad/>","crid":"cr1"}]}]});
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&body))
        .mount(&mock_server)
        .await;

    let exchange = make_exchange_with_bidder(mock_server.uri());

    let mut req = make_simple_request();
    // "1NNN" → index 2 == 'N' → NOT opted out
    req.regs = Some(openrtb::Regs {
        us_privacy: Some("1NNN".to_string()),
        ..Default::default()
    });

    let result = exchange.hold_auction(AuctionRequest {
        bid_request: req, account: None, user_syncs: None,
        start_time: std::time::Instant::now(), currency_rates: None,
    }).await.unwrap();

    assert_eq!(result.bid_response.seatbid.len(), 1, "bid should be returned when CCPA not opted out");
}

// ── Test 17: multi-bid allows multiple bids per imp ─────────────────────────────────────────────

/// A bidder that returns three different bids for the same imp.
struct MultiBidMockBidder;

impl pbs_adapters::Bidder for MultiBidMockBidder {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _: &pbs_adapters::ExtraRequestInfo,
    ) -> (Vec<pbs_adapters::RequestData>, Vec<pbs_adapters::BidderError>) {
        let body = serde_json::to_vec(request).unwrap();
        (vec![pbs_adapters::RequestData::new_post("http://mock", body)], vec![])
    }

    fn make_bids(
        &self,
        _: &openrtb::BidRequest,
        _: &pbs_adapters::RequestData,
        _: &pbs_adapters::ResponseData,
    ) -> Result<pbs_adapters::BidderResponse, Vec<pbs_adapters::BidderError>> {
        let mut resp = pbs_adapters::BidderResponse::new();
        for i in 1u8..=3 {
            let bid = openrtb::Bid {
                id: format!("bid{}", i),
                impid: "imp1".to_string(),
                price: i as f64,
                adm: Some(format!("<ad{i}/>")),
                crid: Some("cr1".to_string()),
                ..Default::default()
            };
            resp.bids.push(pbs_adapters::TypedBid::new(bid, openrtb_ext::BidType::Banner));
        }
        Ok(resp)
    }
}

#[tokio::test]
async fn test_multi_bid_allows_multiple_per_imp() {
    let mut adapters = HashMap::new();
    adapters.insert("appnexus".to_string(), AdaptedBidder {
        bidder: Arc::new(MultiBidMockBidder),
        http_client: reqwest::Client::new(),
        endpoint: "http://mock".to_string(),
        endpoint_compression: None,
    });
    let exchange = Exchange::new(adapters);

    let mut req = make_simple_request();
    // Allow up to 3 bids from appnexus per imp
    req.ext = Some(serde_json::json!({
        "prebid": {
            "multibid": [{"bidder": "appnexus", "maxBids": 3}]
        }
    }));

    let result = exchange.hold_auction(AuctionRequest {
        bid_request: req, account: None, user_syncs: None,
        start_time: std::time::Instant::now(), currency_rates: None,
    }).await.unwrap();

    assert_eq!(result.bid_response.seatbid.len(), 1);
    let bid_count = result.bid_response.seatbid[0].bid.len();
    assert_eq!(bid_count, 3, "multi-bid should allow all 3 bids; got {bid_count}");
}

// ── Test 18: default multi-bid (1) keeps only the highest-priced bid per imp ─────────────────

#[tokio::test]
async fn test_default_single_bid_keeps_highest_price() {
    let mut adapters = HashMap::new();
    adapters.insert("appnexus".to_string(), AdaptedBidder {
        bidder: Arc::new(MultiBidMockBidder),
        http_client: reqwest::Client::new(),
        endpoint: "http://mock".to_string(),
        endpoint_compression: None,
    });
    let exchange = Exchange::new(adapters);

    // No multibid config → default 1 bid per imp
    let result = exchange.hold_auction(AuctionRequest {
        bid_request: make_simple_request(), account: None, user_syncs: None,
        start_time: std::time::Instant::now(), currency_rates: None,
    }).await.unwrap();

    assert_eq!(result.bid_response.seatbid.len(), 1);
    let bids = &result.bid_response.seatbid[0].bid;
    assert_eq!(bids.len(), 1, "default 1-bid mode should keep only one bid");
    assert!(
        (bids[0].price - 3.0).abs() < 1e-9,
        "should keep the highest-priced bid (3.0), got {}",
        bids[0].price
    );
}

// ── Test 19: bid adjustment applied before floor — adjusted bid below floor is filtered ────────

#[tokio::test]
async fn test_bid_adjustment_applied_before_floor() {
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wiremock::matchers::method;

    let mock_server = MockServer::start().await;
    // Raw bid price = 5.0; adjustment factor 0.8 → adjusted = 4.0; floor = 4.5 → rejected
    let body = serde_json::json!({"id":"r","seatbid":[{"bid":[{"id":"b1","impid":"imp1","price":5.0,"adm":"<ad/>","crid":"cr1"}]}]});
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&body))
        .mount(&mock_server)
        .await;

    let exchange = make_exchange_with_bidder(mock_server.uri());

    let mut req = make_simple_request();
    req.imp[0].bidfloor = Some(4.5); // floor is 4.5
    req.ext = Some(serde_json::json!({
        "prebid": {
            "bidadjustmentfactors": {"appnexus": 0.8}
        }
    }));

    let result = exchange.hold_auction(AuctionRequest {
        bid_request: req, account: None, user_syncs: None,
        start_time: std::time::Instant::now(), currency_rates: None,
    }).await.unwrap();

    // Adjusted price 4.0 < floor 4.5 → bid should be filtered out
    assert!(
        result.bid_response.seatbid.is_empty(),
        "bid adjusted below floor should be filtered; seatbids: {:?}",
        result.bid_response.seatbid
    );
    // SeatNonBid with code 300 (below floor) should be emitted
    assert_eq!(result.seat_non_bids.len(), 1);
    assert_eq!(result.seat_non_bids[0].nonbid[0].statuscode, 300);
}

// ── Test 20: price macro ${AUCTION_PRICE} resolved in nurl ──────────────────────────────────

#[tokio::test]
async fn test_price_macro_resolved_in_nurl() {
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wiremock::matchers::method;

    let mock_server = MockServer::start().await;
    let body = serde_json::json!({
        "id": "r",
        "seatbid": [{"bid": [{
            "id": "b1",
            "impid": "imp1",
            "price": 1.23,
            "nurl": "http://track.com?price=${AUCTION_PRICE}",
            "crid": "cr1"
        }]}]
    });
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&body))
        .mount(&mock_server)
        .await;

    let exchange = make_exchange_with_bidder(mock_server.uri());

    let result = exchange.hold_auction(AuctionRequest {
        bid_request: make_simple_request(), account: None, user_syncs: None,
        start_time: std::time::Instant::now(), currency_rates: None,
    }).await.unwrap();

    let nurl = result.bid_response.seatbid[0].bid[0].nurl.as_deref().unwrap_or("");
    assert_eq!(
        nurl, "http://track.com?price=1.2300",
        "nurl AUCTION_PRICE macro should be resolved to 4-decimal price, got: {nurl}"
    );
}

// ── Test 21: price macro ${AUCTION_PRICE} resolved in adm ───────────────────────────────────

#[tokio::test]
async fn test_price_macro_resolved_in_adm() {
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wiremock::matchers::method;

    let mock_server = MockServer::start().await;
    let body = serde_json::json!({
        "id": "r",
        "seatbid": [{"bid": [{
            "id": "b1",
            "impid": "imp1",
            "price": 2.5,
            "adm": "<creative>price=${AUCTION_PRICE}</creative>",
            "crid": "cr1"
        }]}]
    });
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&body))
        .mount(&mock_server)
        .await;

    let exchange = make_exchange_with_bidder(mock_server.uri());

    let result = exchange.hold_auction(AuctionRequest {
        bid_request: make_simple_request(), account: None, user_syncs: None,
        start_time: std::time::Instant::now(), currency_rates: None,
    }).await.unwrap();

    let adm = result.bid_response.seatbid[0].bid[0].adm.as_deref().unwrap_or("");
    assert_eq!(
        adm, "<creative>price=2.5000</creative>",
        "adm AUCTION_PRICE macro should be resolved, got: {adm}"
    );
}

// ── Test 22: multi-bid maxBids=2 keeps top 2 bids of 3 ─────────────────────────────────────

#[tokio::test]
async fn test_multi_bid_max_two_keeps_top_two() {
    let mut adapters = HashMap::new();
    adapters.insert("appnexus".to_string(), AdaptedBidder {
        bidder: Arc::new(MultiBidMockBidder),
        http_client: reqwest::Client::new(),
        endpoint: "http://mock".to_string(),
        endpoint_compression: None,
    });
    let exchange = Exchange::new(adapters);

    let mut req = make_simple_request();
    req.ext = Some(serde_json::json!({
        "prebid": {
            "multibid": [{"bidder": "appnexus", "maxBids": 2}]
        }
    }));

    let result = exchange.hold_auction(AuctionRequest {
        bid_request: req, account: None, user_syncs: None,
        start_time: std::time::Instant::now(), currency_rates: None,
    }).await.unwrap();

    assert_eq!(result.bid_response.seatbid.len(), 1);
    let bids = &result.bid_response.seatbid[0].bid;
    assert_eq!(bids.len(), 2, "maxBids=2 should keep exactly 2 bids; got {}", bids.len());
    // Should be the two highest prices (2.0 and 3.0)
    let prices: Vec<f64> = bids.iter().map(|b| b.price).collect();
    assert!(prices.contains(&3.0), "should contain price 3.0");
    assert!(prices.contains(&2.0), "should contain price 2.0");
}
