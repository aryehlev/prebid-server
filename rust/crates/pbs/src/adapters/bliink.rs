//! Go `adapters/bliink/bliink.go`.

use crate::bid_types::BidType;
use crate::bidder::{
    is_response_status_code_no_content, Bidder, BidderResponse, ExtraRequestInfo, RequestData,
    ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{BidRequest, BidResponse, Imp};

pub struct Adapter {
    endpoint: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self { endpoint: endpoint.into() }
    }
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let body = match crate::go_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::other(e.to_string())]),
        };
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        headers.add("x-openrtb-version", "2.5");
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
        request: &BidRequest,
        _request_data: &RequestData,
        response_data: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if is_response_status_code_no_content(response_data) {
            return (None, vec![]);
        }
        if response_data.status_code == 400 {
            return (
                None,
                vec![BidderError::bad_input(
                    "Unexpected status code: 400. Bad request from publisher. Run with request.debug = 1 for more info.",
                )],
            );
        }
        if response_data.status_code != 200 {
            return (
                None,
                vec![BidderError::bad_server_response(format!(
                    "Unexpected status code: {}. Run with request.debug = 1 for more info.",
                    response_data.status_code
                ))],
            );
        }

        let response: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        if response.seatbid.is_empty() {
            return (None, vec![BidderError::bad_server_response("Empty SeatBid array")]);
        }

        let mut bid_response = BidderResponse::with_bids_capacity(request.imp.len());
        bid_response.currency = response.cur.clone();
        let mut errs = vec![];
        for seat_bid in response.seatbid {
            for bid in seat_bid.bid {
                match get_media_type_for_imp(&bid.impid, &request.imp) {
                    Ok(t) => bid_response.bids.push(TypedBid::new(bid, t)),
                    Err(e) => errs.push(e),
                }
            }
        }
        (Some(bid_response), errs)
    }
}

fn get_media_type_for_imp(imp_id: &str, imps: &[Imp]) -> Result<BidType, BidderError> {
    let mut media_type = BidType::default();
    let mut cnt = 0;
    for imp in imps {
        if imp.id == imp_id {
            if imp.banner.is_some() {
                cnt += 1;
                media_type = BidType::Banner;
            }
            if imp.native.is_some() {
                cnt += 1;
                media_type = BidType::Native;
            }
            if imp.video.is_some() {
                cnt += 1;
                media_type = BidType::Video;
            }
        }
    }
    if cnt == 1 {
        return Ok(media_type);
    }
    Err(BidderError::other(format!("unable to fetch mediaType in multi-format: {imp_id}")))
}
