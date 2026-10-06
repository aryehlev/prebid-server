//! Go `adapters/nobid/nobid.go`.

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
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::bad_input("No Imps in Bid Request")]);
        }
        let body = match crate::go_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::other(e.to_string())]),
        };
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
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
        request: &BidRequest,
        _request_data: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if is_response_status_code_no_content(response) {
            return (None, vec![]);
        }
        let msg = || {
            format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info",
                response.status_code
            )
        };
        if response.status_code == 400 {
            return (None, vec![BidderError::bad_input(msg())]);
        }
        if response.status_code != 200 {
            return (None, vec![BidderError::bad_server_response(msg())]);
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let count: usize = bid_resp.seatbid.iter().map(|sb| sb.bid.len()).sum();
        let mut out = BidderResponse::with_bids_capacity(count);
        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                match get_media_type_for_imp(&bid.impid, &request.imp) {
                    Ok(t) => out.bids.push(TypedBid::new(bid, t)),
                    Err(e) => errs.push(e),
                }
            }
        }
        (Some(out), errs)
    }
}

fn get_media_type_for_imp(imp_id: &str, imps: &[Imp]) -> Result<BidType, BidderError> {
    for imp in imps {
        if imp.id == imp_id {
            if imp.banner.is_none() && imp.video.is_some() {
                return Ok(BidType::Video);
            }
            return Ok(BidType::Banner);
        }
    }
    Err(BidderError::bad_input(format!("Failed to find impression \"{imp_id}\"")))
}
