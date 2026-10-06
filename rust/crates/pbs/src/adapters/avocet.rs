//! Go `adapters/avocet/avocet.go`.

use serde::Deserialize;

use crate::bid_types::{BidType, ExtBidPrebidVideo};
use crate::bidder::{
    is_response_status_code_no_content, Bidder, BidderResponse, ExtraRequestInfo, RequestData,
    ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::adcom1::ApiFramework;
use crate::ortb::openrtb2::{Bid, BidRequest, BidResponse};

pub struct Adapter {
    endpoint: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self { endpoint: endpoint.into() }
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct AvocetBidExt {
    avocet: AvocetBidExtension,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct AvocetBidExtension {
    duration: i32,
    deal_priority: i32,
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![]);
        }
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        let body = match crate::go_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::failed_to_request_bids(e.to_string())]),
        };
        let data = RequestData {
            method: "POST".into(),
            uri: self.endpoint.clone(),
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
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if is_response_status_code_no_content(response) {
            return (None, vec![]);
        }
        if response.status_code != 200 {
            let err_str = if response.body.is_empty() {
                "no response body".to_string()
            } else {
                String::from_utf8_lossy(&response.body).into_owned()
            };
            return (
                None,
                vec![BidderError::bad_server_response(format!(
                    "received status code: {} error: {}",
                    response.status_code, err_str
                ))],
            );
        }
        let br: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![BidderError::bad_server_response(e.to_string())]),
        };

        let mut errs = vec![];
        let mut bid_response = BidderResponse::with_bids_capacity(5);
        for seat_bid in br.seatbid {
            for bid in seat_bid.bid {
                let mut ext = AvocetBidExt::default();
                if let Some(bext) = &bid.ext {
                    match jsonutil::unmarshal::<AvocetBidExt>(bext.to_json().as_bytes()) {
                        Ok(e) => ext = e,
                        Err(e) => {
                            errs.push(e);
                            continue;
                        }
                    }
                }
                let bid_type = get_bid_type(&bid, &ext);
                let mut tbid = TypedBid::new(bid, bid_type);
                tbid.deal_priority = ext.avocet.deal_priority;
                if bid_type == BidType::Video {
                    tbid.bid_video = Some(ExtBidPrebidVideo {
                        duration: ext.avocet.duration,
                        primary_category: String::new(),
                    });
                }
                bid_response.bids.push(tbid);
            }
        }
        if !errs.is_empty() {
            return (None, errs);
        }
        (Some(bid_response), vec![])
    }
}

fn get_bid_type(bid: &Bid, ext: &AvocetBidExt) -> BidType {
    if ext.avocet.duration != 0 {
        return BidType::Video;
    }
    if bid.api == ApiFramework::VPAID_10 || bid.api == ApiFramework::VPAID_20 {
        BidType::Video
    } else {
        BidType::Banner
    }
}
