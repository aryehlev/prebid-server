//! Go `adapters/seedtag/seedtag.go`.

use crate::bid_types::BidType;
use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder,
    BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{Bid, BidRequest, BidResponse};

pub struct Adapter {
    endpoint: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Result<Self, BidderError> {
        Ok(Self { endpoint: endpoint.into() })
    }
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errors = Vec::new();
        let mut imps = Vec::with_capacity(request.imp.len());
        for imp in &request.imp {
            let mut imp = imp.clone();
            if imp.bidfloor > 0.0 && !imp.bidfloorcur.is_empty() && imp.bidfloorcur.to_uppercase() != "USD" {
                match req_info.convert_currency(imp.bidfloor, &imp.bidfloorcur, "USD") {
                    Ok(v) => {
                        imp.bidfloorcur = "USD".into();
                        imp.bidfloor = v;
                    }
                    Err(e) => {
                        errors.push(e);
                        continue;
                    }
                }
            }
            imps.push(imp);
        }
        if imps.is_empty() {
            return (vec![], errors);
        }
        let mut request_copy = request.clone();
        request_copy.imp = imps;
        let body = match crate::go_json::to_vec(&request_copy) {
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
            imp_ids: request_copy.imp.iter().map(|i| i.id.clone()).collect(),
        };
        (vec![data], errors)
    }

    fn make_bids(
        &self,
        _request: &BidRequest,
        request_data: &RequestData,
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
        let mut bid_response = BidderResponse::with_bids_capacity(request_data.imp_ids.len());
        let mut errs = Vec::new();
        for seat_bid in response.seatbid {
            for bid in seat_bid.bid {
                match get_media_type_for_bid(&bid) {
                    Ok(t) => bid_response.bids.push(TypedBid::new(bid, t)),
                    Err(e) => errs.push(e),
                }
            }
        }
        (Some(bid_response), errs)
    }
}

fn get_media_type_for_bid(bid: &Bid) -> Result<BidType, BidderError> {
    match bid.mtype.0 {
        1 => Ok(BidType::Banner),
        2 => Ok(BidType::Video),
        _ => Err(BidderError::other("bid.MType invalid")),
    }
}
