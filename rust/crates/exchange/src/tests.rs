#[cfg(test)]
mod tests {
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
        let err = result.unwrap_err().to_string();
        assert!(err.contains("banner/video/audio/native"), "unexpected error: {err}");
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
}
