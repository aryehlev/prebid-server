//! Go `adapters/coinzilla/coinzilla.go`.

use crate::bid_types::BidType;
use crate::bidder::{
    Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid,
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
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::bad_input("No impression in the bid request")]);
        }
        let body = match crate::go_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::other(e.to_string())]),
        };
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        headers.add("x-openrtb-version", "2.5");
        (
            vec![RequestData {
                method: "POST".into(),
                uri: self.endpoint.clone(),
                body,
                headers,
                imp_ids: request.imp.iter().map(|i| i.id.clone()).collect(),
            }],
            vec![],
        )
    }

    fn make_bids(
        &self,
        _request: &BidRequest,
        _request_data: &RequestData,
        response_data: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        match response_data.status_code {
            200 => {}
            204 => return (None, vec![]),
            code => {
                return (
                    None,
                    vec![BidderError::bad_server_response(format!(
                        "Unexpected code: {code}. Run with request.debug = 1"
                    ))],
                )
            }
        }
        let response: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        // Go indexes `SeatBid[0]` and panics on an empty seatbid; report an error instead.
        let Some(first) = response.seatbid.first() else {
            return (None, vec![BidderError::bad_server_response("no seatbid in the response")]);
        };
        let mut out = BidderResponse::with_bids_capacity(first.bid.len());
        out.currency = response.cur.clone();
        for seat_bid in response.seatbid {
            for bid in seat_bid.bid {
                out.bids.push(TypedBid::new(bid, BidType::Banner));
            }
        }
        (Some(out), vec![])
    }
}
