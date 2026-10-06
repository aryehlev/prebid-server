//! Go `adapters/smartx/smartx.go`.

use crate::bid_types::BidType;
use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder, BidderResponse,
    ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{BidRequest, BidResponse};

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
            Err(e) => return (vec![], vec![BidderError::other(format!("marshal bidRequest: {e}"))]),
        };

        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("x-openrtb-version", "2.5");

        if let Some(device) = &request.device {
            if !device.ua.is_empty() {
                headers.set("User-Agent", device.ua.clone());
            }
            if !device.ip.is_empty() {
                headers.set("Forwarded", format!("for={}", device.ip));
                headers.set("X-Forwarded-For", device.ip.clone());
            }
        }

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
        if let Some(err) = check_response_status_code_for_errors(response_data) {
            return (None, vec![err]);
        }
        let response: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        if response.seatbid.is_empty() {
            return (None, vec![BidderError::other("no bidders found in JSON response")]);
        }

        let mut bid_response = BidderResponse::with_bids_capacity(request.imp.len());
        if !response.cur.is_empty() {
            bid_response.currency = response.cur.clone();
        }
        for seat_bid in response.seatbid {
            for bid in seat_bid.bid {
                bid_response.bids.push(TypedBid::new(bid, BidType::Video));
            }
        }
        (Some(bid_response), vec![])
    }
}
