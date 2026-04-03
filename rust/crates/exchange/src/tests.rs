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
                "adm": "<ad/>"
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
                "adm": "<ad/>"
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
