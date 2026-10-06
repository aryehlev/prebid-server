//! Go `adapters/dxkulture/dxkulture.go`.

#![allow(unused_imports, dead_code)]

use serde::{Deserialize, Serialize};

use crate::bid_types::BidType;
use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder, BidderResponse,
    ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::macros::{EndpointTemplate, EndpointTemplateParams};
use crate::ortb::openrtb2::{BidRequest, BidResponse, Imp, MarkupType};
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
struct ExtImpDxKulture {
    #[serde(rename = "publisherId")]
    publisher_id: String,
    #[serde(rename = "placementId")]
    placement_id: String,
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

/// Go `url.QueryEscape`.
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

fn parse_ext(imp: &Imp) -> Result<ExtImpDxKulture, BidderError> {
    let outer: ExtImpBidder = unmarshal_raw(&ext_bytes(&imp.ext)).map_err(|e| {
        BidderError::bad_input(format!(
            "Ignoring imp id={}, error while decoding extImpBidder, err: {}",
            imp.id, e
        ))
    })?;
    unmarshal_raw(&ext_bytes(&outer.bidder)).map_err(|e| {
        BidderError::bad_input(format!("Ignoring imp id={}, error while decoding impExt, err: {}", imp.id, e))
    })
}

fn get_headers(request: &BidRequest) -> Header {
    let mut headers = Header::new();
    headers.add("Content-Type", "application/json;charset=utf-8");
    headers.add("Accept", "application/json");
    headers.add("X-Openrtb-Version", "2.5");
    if let Some(site) = &request.site {
        if !site.r#ref.is_empty() {
            headers.set("Referer", site.r#ref.clone());
        }
        if !site.domain.is_empty() {
            headers.add("Origin", site.domain.clone());
        }
    }
    if let Some(device) = &request.device {
        if !device.ua.is_empty() {
            headers.add("User-Agent", device.ua.clone());
        }
        if !device.ipv6.is_empty() {
            headers.add("X-Forwarded-For", device.ipv6.clone());
        }
        if !device.ip.is_empty() {
            headers.add("X-Forwarded-For", device.ip.clone());
        }
    }
    headers
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut reqs = Vec::with_capacity(request.imp.len());
        let mut errs = Vec::new();
        // Go swaps `request.Imp` in place; a shallow copy without the imps does the same here.
        let mut single = request.clone();
        for imp in &request.imp {
            let mut ext = match parse_ext(imp) {
                Ok(e) => e,
                Err(e) => {
                    errs.push(e);
                    continue;
                }
            };
            single.imp = vec![imp.clone()];
            let body = match crate::go_json::to_vec(&single) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(BidderError::other(e.to_string()));
                    continue;
                }
            };
            if request.test == 1 {
                ext.publisher_id = "test".into();
            }
            // `url.Values.Encode` sorts by key.
            let query = format!(
                "placement_id={}&publisher_id={}",
                query_escape(&ext.placement_id),
                query_escape(&ext.publisher_id)
            );
            reqs.push(RequestData {
                method: "POST".into(),
                uri: format!("{}?{}", self.endpoint, query),
                body,
                headers: get_headers(request),
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
        let response: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(_) => return (None, vec![BidderError::bad_server_response("Bad Server Response")]),
        };
        let mut errs = Vec::new();
        let mut out = BidderResponse::with_bids_capacity(1);
        for seat_bid in response.seatbid {
            for bid in seat_bid.bid {
                match bid.mtype {
                    MarkupType::BANNER => out.bids.push(TypedBid::new(bid, BidType::Banner)),
                    MarkupType::VIDEO => out.bids.push(TypedBid::new(bid, BidType::Video)),
                    m => errs.push(BidderError::bad_server_response(format!("Unsupported MType {}", m.0))),
                }
            }
        }
        (Some(out), errs)
    }
}
