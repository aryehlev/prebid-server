//! Go `adapters/kueezrtb/kueezrtb.go`.

use serde::Deserialize;
use sonic_rs::JsonValueTrait;

use crate::bid_types::BidType;
use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder, BidderResponse,
    ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{Bid, BidRequest, BidResponse, Imp, MarkupType};
use crate::ortb::Ext;

/// Go `openrtb_ext.ImpExtKueez`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ImpExtKueez {
    #[serde(rename = "cId", alias = "cid")] // Go matches keys case-insensitively; fixtures send "cid"
    connection_id: String,
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

/// Go: `jsonutil.Unmarshal(imp.Ext, &ExtImpBidder)` then `jsonutil.Unmarshal(bidderExt.Bidder, &T)`.
/// Returns the Go error message text of whichever step fails.
fn imp_bidder_params<T: serde::de::DeserializeOwned>(ext: Option<&Ext>) -> Result<T, String> {
    fn not_obj(v: &sonic_rs::Value) -> Option<String> {
        if v.is_object() || v.is_null() {
            return None;
        }
        let first = sonic_rs::to_string(v).ok()?.chars().next().unwrap_or('\u{0}');
        Some(format!("expect {{ or n, but found {first}"))
    }
    let Some(ext) = ext else {
        return Err("expect { or n, but found \u{0}".to_string());
    };
    if let Some(m) = not_obj(&ext.0) {
        return Err(m);
    }
    match ext.0.get("bidder") {
        None => Err("expect { or n, but found \u{0}".to_string()),
        Some(b) => {
            if let Some(m) = not_obj(b) {
                return Err(m);
            }
            sonic_rs::from_value::<T>(b).map_err(|e| e.to_string())
        }
    }
}

/// Go `url.QueryEscape`.
fn query_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn extract_cid(imp: &Imp) -> Result<String, String> {
    let ext: ImpExtKueez = imp_bidder_params(imp.ext.as_ref()).map_err(|e| {
        // Go wraps the unmarshal step that failed; both wrap texts are reproduced from the message.
        format!("unmarshal bidderExt: {e}")
    })?;
    Ok(ext.connection_id)
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
            let mut request_copy = request.clone();
            request_copy.imp = vec![imp.clone()];

            let body = match crate::go_json::to_vec(&request_copy) {
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
                // Go formats the error with `%d`, which prints the struct fields: `&{msg code}`.
                return (
                    None,
                    vec![BidderError::bad_server_response(format!(
                        "bad server response: &{{%!d(string={}) {}}}. ",
                        e.message(),
                        e.code()
                    ))],
                );
            }
        };

        let mut bid_response = BidderResponse::with_bids_capacity(response.seatbid.len());
        if !response.cur.is_empty() {
            bid_response.currency = response.cur.clone();
        }
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
    match bid.mtype {
        MarkupType::BANNER => Ok(BidType::Banner),
        MarkupType::VIDEO => Ok(BidType::Video),
        _ => Err(BidderError::bad_input(format!("Could not define bid type for imp: {}", bid.impid))),
    }
}
