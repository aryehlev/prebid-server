//! Go `adapters/bluesea/bluesea.go`.

use serde::Deserialize;

use crate::bid_types::BidType;
use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder, BidderResponse,
    ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{BidRequest, BidResponse, Imp};
use crate::ortb::Ext;

/// Go `adapters.ExtImpBidder`.
#[derive(Deserialize, Default)]
struct ExtImpBidder {
    #[serde(default)]
    bidder: Option<Ext>,
}

fn ext_bytes(ext: &Option<Ext>) -> Vec<u8> {
    ext.as_ref().map(|e| e.to_json().into_bytes()).unwrap_or_default()
}

/// `jsonutil.Unmarshal` of an absent `json.RawMessage`: jsoniter reads a NUL byte and reports
/// `expect { or n, but found \u{0}` where serde would say EOF.
fn unmarshal_raw<T: serde::de::DeserializeOwned>(data: &[u8]) -> Result<T, BidderError> {
    if data.iter().all(|b| b" \t\r\n".contains(b)) {
        return Err(BidderError::FailedToUnmarshal("expect { or n, but found \u{0}".to_string()));
    }
    jsonutil::unmarshal(data)
}

/// `Unmarshal(imp.Ext, &bidderExt)` then `Unmarshal(bidderExt.Bidder, &params)`.
fn parse_bidder_ext<T: serde::de::DeserializeOwned>(imp: &Imp) -> Result<T, BidderError> {
    let outer: ExtImpBidder = unmarshal_raw(&ext_bytes(&imp.ext))?;
    unmarshal_raw(&ext_bytes(&outer.bidder))
}

fn json_headers() -> Header {
    let mut headers = Header::new();
    headers.add("Content-Type", "application/json;charset=utf-8");
    headers.add("Accept", "application/json");
    headers
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpBluesea {
    pubid: String,
    token: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct BlueseaBidExt {
    mediatype: String,
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

/// Go `url.Values.Encode` / `QueryEscape` for one value.
fn query_escape(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn extra_imp_ext(imp: &Imp) -> Result<ExtImpBluesea, BidderError> {
    let raw = ext_bytes(&imp.ext);
    let outer: ExtImpBidder = jsonutil::unmarshal(&raw).map_err(|e| {
        BidderError::bad_input(format!(
            "Error in parsing imp.ext. err = {}, imp.ext = {}",
            e,
            String::from_utf8_lossy(&raw)
        ))
    })?;
    let bidder_raw = ext_bytes(&outer.bidder);
    let ext: ExtImpBluesea = jsonutil::unmarshal(&bidder_raw).map_err(|e| {
        BidderError::bad_input(format!(
            "Error in parsing imp.ext.bidder. err = {}, bidder = {}",
            e,
            String::from_utf8_lossy(&bidder_raw)
        ))
    })?;
    if ext.pubid.is_empty() || ext.token.is_empty() {
        return Err(BidderError::bad_input("Error in parsing imp.ext.bidder, empty pubid or token"));
    }
    Ok(ext)
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::bad_input("Empty Imp objects")]);
        }
        let mut datas = Vec::with_capacity(request.imp.len());
        let mut errs = Vec::new();
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        // Go marshals the same request for every imp; serialize it once.
        let mut body: Option<Result<Vec<u8>, BidderError>> = None;
        for imp in &request.imp {
            let ext = match extra_imp_ext(imp) {
                Ok(e) => e,
                Err(e) => {
                    errs.push(e);
                    continue;
                }
            };
            let body = body
                .get_or_insert_with(|| {
                    crate::go_json::to_vec(request).map_err(|e| BidderError::other(e.to_string()))
                })
                .clone();
            let body = match body {
                Ok(b) => b,
                Err(e) => {
                    errs.push(e);
                    continue;
                }
            };
            let query = format!("pubid={}&token={}", query_escape(&ext.pubid), query_escape(&ext.token));
            datas.push(RequestData {
                method: "POST".into(),
                uri: format!("{}?{}", self.endpoint, query),
                body,
                headers: headers.clone(),
                imp_ids: request.imp.iter().map(|i| i.id.clone()).collect(),
            });
        }
        if datas.is_empty() && errs.is_empty() {
            errs.push(BidderError::other("Empty RequestData"));
        }
        (datas, errs)
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
        let response: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(_) => return (None, vec![BidderError::other("Error in parsing bidresponse body")]),
        };
        let mut errs = Vec::new();
        let mut bid_response = BidderResponse::with_bids_capacity(1);
        if !response.cur.is_empty() {
            bid_response.currency = response.cur.clone();
        }
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

fn get_media_type_for_bid(bid: &crate::ortb::openrtb2::Bid) -> Result<BidType, BidderError> {
    let raw = ext_bytes(&bid.ext);
    let ext: BlueseaBidExt =
        jsonutil::unmarshal(&raw).map_err(|_| BidderError::other("Error in parsing bid.ext"))?;
    match ext.mediatype.as_str() {
        "banner" => Ok(BidType::Banner),
        "native" => Ok(BidType::Native),
        "video" => Ok(BidType::Video),
        other => Err(BidderError::other(format!("Unknown bid type, {other}"))),
    }
}
