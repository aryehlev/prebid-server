//! Go `adapters/tpmn/tpmn.go`.

use crate::bid_types::BidType;
use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder,
    BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{Bid, BidRequest, BidResponse, Imp, MarkupType};

pub struct Adapter {
    uri: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self { uri: endpoint.into() }
    }
}

fn preprocess_bid_floor_currency(imp: &mut Imp, req_info: &ExtraRequestInfo) -> Result<(), BidderError> {
    if imp.bidfloor > 0.0 && imp.bidfloorcur.to_uppercase() != "USD" && !imp.bidfloorcur.is_empty() {
        imp.bidfloor = req_info.convert_currency(imp.bidfloor, &imp.bidfloorcur, "USD")?;
    }
    imp.bidfloorcur = "USD".into();
    Ok(())
}

fn get_media_type_for_imp(bid: &Bid) -> Result<BidType, BidderError> {
    match bid.mtype {
        MarkupType::BANNER => Ok(BidType::Banner),
        MarkupType::VIDEO => Ok(BidType::Video),
        MarkupType::NATIVE => Ok(BidType::Native),
        other => Err(BidderError::other(format!("unsupported MType {}", other.0))),
    }
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut valid_imps = Vec::new();
        for imp in &request.imp {
            let mut imp = imp.clone();
            match preprocess_bid_floor_currency(&mut imp, req_info) {
                Ok(()) => valid_imps.push(imp),
                Err(e) => errs.push(e),
            }
        }
        if valid_imps.is_empty() {
            return (vec![], errs);
        }
        // Go mutates `request.Imp` in place; a clone keeps the caller's request untouched.
        let mut copy = request.clone();
        copy.imp = valid_imps;
        let body = match crate::go_json::to_vec(&copy) {
            Ok(b) => b,
            Err(e) => {
                errs.push(BidderError::other(e.to_string()));
                return (vec![], errs);
            }
        };
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        (
            vec![RequestData {
                method: "POST".into(),
                uri: self.uri.clone(),
                body,
                headers,
                imp_ids: copy.imp.iter().map(|i| i.id.clone()).collect(),
            }],
            errs,
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
        let response: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![BidderError::other(format!("bid response unmarshal: {e}"))]),
        };
        let mut out = BidderResponse::with_bids_capacity(request.imp.len());
        out.currency = response.cur.clone();
        for sb in response.seatbid {
            for bid in sb.bid {
                match get_media_type_for_imp(&bid) {
                    Ok(t) => out.bids.push(TypedBid::new(bid, t)),
                    Err(e) => return (None, vec![e]),
                }
            }
        }
        (Some(out), vec![])
    }
}
