//! Go `adapters/vidazoo/vidazoo.go`.

use serde::Deserialize;

use crate::bid_types::BidType;
use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder,
    BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{Bid, BidRequest, BidResponse, Imp, MarkupType};
use crate::ortb::Ext;

pub struct Adapter {
    endpoint: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self { endpoint: endpoint.into() }
    }
}

#[derive(Deserialize, Default)]
struct ExtImpBidder {
    #[serde(default)]
    bidder: Option<Ext>,
}

fn extract_cid(imp: &Imp) -> Result<String, BidderError> {
    let Some(ext) = &imp.ext else {
        return Err(BidderError::other("unmarshal bidderExt: unexpected end of JSON input"));
    };
    let bidder_ext: ExtImpBidder = ext
        .decode()
        .map_err(|e| BidderError::other(format!("unmarshal bidderExt: {e}")))?;
    let Some(bidder) = bidder_ext.bidder else {
        return Err(BidderError::other("unmarshal ImpExtVidazoo: unexpected end of JSON input"));
    };
    // Go matches JSON keys case-insensitively (`cid` fills `json:"cId"`); the last match wins.
    let map: std::collections::BTreeMap<String, serde_json::Value> = bidder
        .decode()
        .map_err(|e| BidderError::other(format!("unmarshal ImpExtVidazoo: {e}")))?;
    let mut cid = String::new();
    for (k, v) in &map {
        if k.eq_ignore_ascii_case("cId") {
            match v {
                serde_json::Value::String(s) => cid = s.clone(),
                serde_json::Value::Null => {}
                _ => {
                    return Err(BidderError::other(
                        "unmarshal ImpExtVidazoo: json: cannot unmarshal into Go struct field ImpExtVidazoo.cId of type string",
                    ))
                }
            }
        }
    }
    Ok(cid.trim().to_string())
}

fn get_media_type_for_bid(bid: &Bid) -> Result<BidType, BidderError> {
    match bid.mtype {
        MarkupType::BANNER => Ok(BidType::Banner),
        MarkupType::VIDEO => Ok(BidType::Video),
        _ => Err(BidderError::bad_input(format!(
            "Could not define bid type for imp: {}",
            bid.impid
        ))),
    }
}

/// Go `url.QueryEscape`.
fn query_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();
        let mut errors = Vec::new();
        for imp in &request.imp {
            let mut copy = request.clone();
            copy.imp = vec![imp.clone()];
            let body = match crate::go_json::to_vec(&copy) {
                Ok(b) => b,
                Err(e) => {
                    errors.push(BidderError::other(format!("marshal bidRequest: {e}")));
                    continue;
                }
            };
            let cid = match extract_cid(imp) {
                Ok(c) => c,
                Err(e) => {
                    errors.push(BidderError::other(format!("extract cId: {e}")));
                    continue;
                }
            };
            let mut headers = Header::new();
            headers.add("Content-Type", "application/json;charset=utf-8");
            requests.push(RequestData {
                method: "POST".into(),
                uri: format!("{}{}", self.endpoint, query_escape(&cid)),
                body,
                headers,
                imp_ids: vec![imp.id.clone()],
            });
        }
        (requests, errors)
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
        if check_response_status_code_for_errors(response_data).is_some() {
            return (
                None,
                vec![BidderError::bad_input(format!(
                    "Unexpected status code: {}. Run with request.debug = 1 for more info",
                    response_data.status_code
                ))],
            );
        }
        let response: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(e) => {
                // Go formats the error with `%d`, which prints `%!d(...)` noise; keep the prefix.
                return (
                    None,
                    vec![BidderError::bad_server_response(format!("bad server response: {}. ", fmt_d_error(&e)))],
                );
            }
        };
        let mut out = BidderResponse::with_bids_capacity(response.seatbid.len());
        if !response.cur.is_empty() {
            out.currency = response.cur.clone();
        }
        let mut errs = Vec::new();
        for sb in response.seatbid {
            for bid in sb.bid {
                match get_media_type_for_bid(&bid) {
                    Ok(t) => out.bids.push(TypedBid::new(bid, t)),
                    Err(e) => errs.push(e),
                }
            }
        }
        (Some(out), errs)
    }
}

/// Go `fmt.Sprintf("%d", err)` on an `*errortypes.FailedToUnmarshal` (a struct pointer with a
/// string field): `&{%!d(string=<message>)}`.
fn fmt_d_error(e: &BidderError) -> String {
    format!("&{{%!d(string={})}}", e.message())
}
