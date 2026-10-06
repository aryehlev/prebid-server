//! Go `adapters/loopme/loopme.go`.

use crate::bid_types::BidType;
use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder,
    BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::ortb::openrtb2::{Bid, BidRequest, BidResponse, MarkupType};

pub struct Adapter {
    endpoint: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self { endpoint: endpoint.into() }
    }
}

fn get_bid_type(bid: &Bid) -> Result<BidType, BidderError> {
    match bid.mtype {
        MarkupType::BANNER => Ok(BidType::Banner),
        MarkupType::VIDEO => Ok(BidType::Video),
        MarkupType::NATIVE => Ok(BidType::Native),
        MarkupType::AUDIO => Ok(BidType::Audio),
        other => Err(BidderError::bad_server_response(format!("Unsupported MType {}", other.0))),
    }
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut reqs = Vec::new();
        for imp in &request.imp {
            let mut copy = request.clone();
            copy.imp = vec![imp.clone()];
            let body = match crate::go_json::to_vec(&copy) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(BidderError::other(e.to_string()));
                    continue;
                }
            };
            let mut headers = Header::new();
            headers.add("Content-Type", "application/json;charset=utf-8");
            headers.add("Accept", "application/json");
            reqs.push(RequestData {
                method: "POST".into(),
                uri: self.endpoint.clone(),
                body,
                headers,
                imp_ids: vec![imp.id.clone()],
            });
        }
        (reqs, errs)
    }

    fn make_bids(
        &self,
        _request: &BidRequest,
        _request_data: &RequestData,
        response_data: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if is_response_status_code_no_content(response_data) {
            return (None, vec![]);
        }
        if let Some(err) = check_response_status_code_for_errors(response_data) {
            return (None, vec![err]);
        }
        let resp: BidResponse = match go_std_unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        if resp.seatbid.is_empty() || resp.seatbid[0].bid.is_empty() {
            return (None, vec![]);
        }
        let mut errs = Vec::new();
        let mut out = BidderResponse::with_bids_capacity(resp.seatbid[0].bid.len());
        for sb in resp.seatbid {
            for bid in sb.bid {
                match get_bid_type(&bid) {
                    Ok(t) => out.bids.push(TypedBid::new(bid, t)),
                    Err(e) => errs.push(e),
                }
            }
        }
        if out.bids.is_empty() {
            return (None, errs);
        }
        if !resp.cur.is_empty() {
            out.currency = resp.cur;
        }
        (Some(out), errs)
    }
}

/// Go `encoding/json.Unmarshal` into `openrtb2.BidResponse` (this adapter does not use
/// `jsonutil`): a top-level non-object value is `json: cannot unmarshal {kind} into Go value of
/// type openrtb2.BidResponse`.
fn go_std_unmarshal(data: &[u8]) -> Result<BidResponse, BidderError> {
    let value: serde_json::Value = serde_json::from_slice(data)
        .map_err(|e| BidderError::other(e.to_string()))?;
    let kind = match &value {
        serde_json::Value::Object(_) | serde_json::Value::Null => "",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::Bool(_) => "bool",
        serde_json::Value::Array(_) => "array",
    };
    if !kind.is_empty() {
        return Err(BidderError::other(format!(
            "json: cannot unmarshal {kind} into Go value of type openrtb2.BidResponse"
        )));
    }
    serde_json::from_value(value).map_err(|e| BidderError::other(e.to_string()))
}
