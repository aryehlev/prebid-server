//! Go `adapters/intertech/intertech.go`.

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

use crate::ortb::openrtb2::Banner;

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

const PAGE_ID_MACRO: &str = "{{page_id}}";
const IMP_ID_MACRO: &str = "{{imp_id}}";

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpIntertech {
    page_id: i64,
    imp_id: i64,
}

pub struct Adapter {
    endpoint: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self { endpoint: endpoint.into() }
    }

    fn modify_url(&self, ext: &ExtImpIntertech, referer: &str, cur: &str) -> String {
        let mut url = self.endpoint.replace(PAGE_ID_MACRO, &query_escape(&ext.page_id.to_string()));
        url = url.replace(IMP_ID_MACRO, &query_escape(&ext.imp_id.to_string()));
        if !referer.is_empty() {
            url.push_str("&target-ref=");
            url.push_str(&query_escape(referer));
        }
        if !cur.is_empty() {
            url.push_str("&ssp-cur=");
            url.push_str(cur);
        }
        url
    }
}

fn parse_and_validate_imp_ext(imp: &Imp) -> Result<ExtImpIntertech, BidderError> {
    let outer: ExtImpBidder = unmarshal_raw(&ext_bytes(&imp.ext))
        .map_err(|e| BidderError::bad_input(format!("imp #{}: unable to parse bidder ext: {}", imp.id, e)))?;
    unmarshal_raw(&ext_bytes(&outer.bidder))
        .map_err(|e| BidderError::bad_input(format!("imp #{}: unable to parse intertech ext: {}", imp.id, e)))
}

fn update_banner(banner: &Banner) -> Result<Banner, String> {
    let mut copy = banner.clone();
    let missing = |v: Option<i64>| v.map_or(true, |x| x == 0);
    if missing(copy.w) || missing(copy.h) {
        if let Some(first) = copy.format.first() {
            copy.w = Some(first.w);
            copy.h = Some(first.h);
        } else {
            return Err("Invalid sizes provided for Banner".into());
        }
    }
    Ok(copy)
}

fn modify_imp(imp: &Imp) -> Result<Imp, BidderError> {
    let mut imp = imp.clone();
    if let Some(banner) = &imp.banner {
        let banner = update_banner(banner)
            .map_err(|e| BidderError::bad_input(format!("imp #{}: {}", imp.id, e)))?;
        imp.banner = Some(banner);
    }
    Ok(imp)
}

fn build_request_data(request: &BidRequest, uri: String) -> Result<RequestData, BidderError> {
    let body = crate::go_json::to_vec(request).map_err(|e| BidderError::other(e.to_string()))?;
    let mut headers = json_headers();
    if let Some(device) = &request.device {
        if !device.ua.is_empty() {
            headers.add("User-Agent", device.ua.clone());
        }
        if !device.ip.is_empty() {
            headers.add("X-Forwarded-For", device.ip.clone());
            headers.add("X-Real-Ip", device.ip.clone());
        }
        if !device.language.is_empty() {
            headers.add("Accept-Language", device.language.clone());
        }
    }
    Ok(RequestData {
        method: "POST".into(),
        uri,
        body,
        headers,
        imp_ids: request.imp.iter().map(|i| i.id.clone()).collect(),
    })
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut requests = Vec::new();
        let referer = request.site.as_ref().map_or("", |s| s.page.as_str());
        let cur = request.cur.first().map_or("", String::as_str);
        let mut mod_request = request.clone();
        for imp in &request.imp {
            let ext = match parse_and_validate_imp_ext(imp) {
                Ok(e) => e,
                Err(e) => {
                    errs.push(e);
                    continue;
                }
            };
            let modified = match modify_imp(imp) {
                Ok(i) => i,
                Err(e) => {
                    errs.push(e);
                    continue;
                }
            };
            let url = self.modify_url(&ext, referer, cur);
            mod_request.imp = vec![modified];
            match build_request_data(&mod_request, url) {
                Ok(r) => requests.push(r),
                Err(e) => errs.push(e),
            }
        }
        (requests, errs)
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
            Err(e) => return (None, vec![e]),
        };
        let mut out = BidderResponse::with_bids_capacity(request.imp.len());
        if !response.cur.is_empty() {
            out.currency = response.cur.clone();
        }
        for seat_bid in response.seatbid {
            for bid in seat_bid.bid {
                let t = match bid.mtype {
                    MarkupType::BANNER => BidType::Banner,
                    MarkupType::NATIVE => BidType::Native,
                    _ => {
                        return (
                            None,
                            vec![BidderError::other(format!(
                                "could not define media type for impression: {}",
                                bid.impid
                            ))],
                        )
                    }
                };
                out.bids.push(TypedBid::new(bid, t));
            }
        }
        (Some(out), vec![])
    }
}
