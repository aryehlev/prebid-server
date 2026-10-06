//! Go `adapters/revcontent/revcontent.go`.

use crate::bid_types::BidType;
use crate::bidder::{Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
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

fn check_request(request: &BidRequest) -> Result<(), BidderError> {
    let has_app = request.app.as_ref().is_some_and(|a| !a.name.is_empty());
    let has_site = request.site.as_ref().is_some_and(|s| !s.domain.is_empty());
    if !has_app && !has_site {
        return Err(BidderError::bad_input("Impression is missing app name or site domain, and must contain one."));
    }
    Ok(())
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
        if let Err(e) = check_request(request) {
            return (vec![], vec![e]);
        }
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
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
        if response.status_code == 204 {
            return (None, vec![]);
        }
        if response.status_code == 400 {
            return (None, vec![BidderError::bad_input(format!("unexpected status code: {}.", response.status_code))]);
        }
        if response.status_code != 200 {
            return (
                None,
                vec![BidderError::bad_server_response(format!("unexpected status code: {}.", response.status_code))],
            );
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut bid_response = BidderResponse::with_bids_capacity(5);
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let t = get_bid_type(&bid.adm);
                bid_response.bids.push(TypedBid::new(bid, t));
            }
        }
        (Some(bid_response), vec![])
    }
}

fn get_bid_type(adm: &str) -> BidType {
    if adm.starts_with('<') {
        BidType::Banner
    } else {
        BidType::Native
    }
}
