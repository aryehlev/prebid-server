//! Go `adapters/pwbid/pwbid.go`.

use crate::bid_types::BidType;
use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder,
    BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{Bid, BidRequest, BidResponse, Imp};

pub struct Adapter {
    endpoint: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self { endpoint: endpoint.into() }
    }
}

fn get_media_type_for_bid(imps: &[Imp], bid: &Bid) -> Result<BidType, BidderError> {
    for imp in imps {
        if imp.id == bid.impid {
            if imp.banner.is_some() {
                return Ok(BidType::Banner);
            }
            if imp.native.is_some() {
                return Ok(BidType::Native);
            }
            if imp.video.is_some() {
                return Ok(BidType::Video);
            }
        }
    }
    Err(BidderError::bad_server_response(format!(
        "The impression with ID {} is not present into the request",
        bid.impid
    )))
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
        (
            vec![RequestData {
                method: "POST".into(),
                uri: self.endpoint.clone(),
                body,
                headers: Header::new(),
                imp_ids: request.imp.iter().map(|i| i.id.clone()).collect(),
            }],
            vec![],
        )
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
        let response: BidResponse = match unmarshal_response(&response_data.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut out = BidderResponse::with_bids_capacity(request.imp.len());
        let mut errors = Vec::new();
        for sb in response.seatbid {
            for bid in sb.bid {
                match get_media_type_for_bid(&request.imp, &bid) {
                    Ok(t) => out.bids.push(TypedBid::new(bid, t)),
                    Err(e) => errors.push(e),
                }
            }
        }
        (Some(out), errors)
    }
}

/// `jsonutil::unmarshal` plus json-iterator's wording for an empty body
/// (`expect { or n, but found \x00`), which the shared helper reports in serde's words.
fn unmarshal_response(data: &[u8]) -> Result<BidResponse, BidderError> {
    if data.iter().all(|b| b" \t\r\n".contains(b)) {
        return Err(BidderError::FailedToUnmarshal("expect { or n, but found \u{0}".into()));
    }
    jsonutil::unmarshal(data)
}
