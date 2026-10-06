//! Go `adapters/sa_lunamedia/salunamedia.go`.

use crate::bid_types::BidType;
use crate::bidder::{Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{BidRequest, BidResponse};
use serde::Deserialize;

/// Go `openrtb_ext.ExtImpSaLunamedia` (never read by the adapter).
#[derive(Debug, Default, Deserialize)]
#[allow(dead_code)]
pub struct ExtImpSaLunamedia {
    pub key: String,
    pub r#type: String,
}

#[derive(Debug, Default, Deserialize)]
struct BidExt {
    #[serde(rename = "mediaType", default)]
    media_type: String,
}

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
        response_data: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        let code = response_data.status_code;
        let body_text = || String::from_utf8_lossy(&response_data.body).into_owned();
        if code == 204 {
            return (None, vec![]);
        }
        if code == 400 {
            return (None, vec![BidderError::bad_input(format!("Bad Request. {}", body_text()))]);
        }
        if code == 503 {
            return (None, vec![BidderError::bad_input("Bidder unavailable. Please contact the bidder support.")]);
        }
        if code != 200 {
            return (None, vec![BidderError::bad_server_response(format!("Status Code: [ {code} ] {}", body_text()))]);
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let Some(seat) = bid_resp.seatbid.into_iter().next() else {
            return (None, vec![BidderError::bad_server_response("Empty SeatBid")]);
        };
        let Some(bid) = seat.bid.into_iter().next() else {
            return (None, vec![BidderError::bad_server_response("Empty SeatBid.Bids")]);
        };
        let bid_ext: BidExt = match bid.ext.as_ref().map(|e| e.decode::<BidExt>()) {
            Some(Ok(v)) => v,
            _ => return (None, vec![BidderError::bad_server_response("Missing BidExt")]),
        };
        let bid_type = match BidType::parse(&bid_ext.media_type) {
            Ok(t) => t,
            Err(e) => return (None, vec![BidderError::other(e)]),
        };
        let mut bid_response = BidderResponse::with_bids_capacity(1);
        bid_response.bids.push(TypedBid::new(bid, bid_type));
        (Some(bid_response), vec![])
    }
}
