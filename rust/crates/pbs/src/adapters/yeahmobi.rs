//! Go `adapters/yeahmobi/yeahmobi.go`.

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

use crate::bid_types::ExtBidPrebidVideo;

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

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
struct ExtImpYeahmobi {
    #[serde(rename = "pubId")]
    pub_id: String,
    #[serde(rename = "zoneId")]
    zone_id: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct YeahmobiBidExt {
    video: Option<YeahmobiBidExtVideo>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct YeahmobiBidExtVideo {
    duration: Option<i64>,
}

pub struct Adapter {
    endpoint_template: EndpointTemplate,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl AsRef<str>) -> Result<Self, BidderError> {
        let endpoint_template = EndpointTemplate::parse(endpoint.as_ref())
            .map_err(|e| BidderError::other(format!("unable to parse endpoint url template: {e}")))?;
        Ok(Self { endpoint_template })
    }
}

/// Go `getYeahmobiExt`: the last successfully parsed ext (the loop breaks at the first), plus the
/// errors of the imps tried before it. Never nil in Go, so an all-failing request gets a zero ext.
fn get_yeahmobi_ext(request: &BidRequest) -> (ExtImpYeahmobi, Vec<BidderError>) {
    let mut ext = ExtImpYeahmobi::default();
    let mut errs = Vec::new();
    for imp in &request.imp {
        let outer: ExtImpBidder = match unmarshal_raw(&ext_bytes(&imp.ext)) {
            Ok(o) => o,
            Err(e) => {
                errs.push(e);
                continue;
            }
        };
        match unmarshal_raw::<ExtImpYeahmobi>(&ext_bytes(&outer.bidder)) {
            Ok(e) => {
                ext = e;
                break;
            }
            Err(e) => {
                errs.push(e);
                continue;
            }
        }
    }
    (ext, errs)
}

/// Go `transform`: wraps each native request in `{"native": ...}` unless it already is.
fn transform(request: &mut BidRequest) {
    for imp in request.imp.iter_mut() {
        let Some(native) = imp.native.as_mut() else { continue };
        // just ignore the bad native request
        let Ok(native_request) =
            jsonutil::unmarshal::<std::collections::BTreeMap<String, serde_json::Value>>(native.request.as_bytes())
        else {
            continue;
        };
        if native_request.contains_key("native") {
            continue;
        }
        let mut copy = std::collections::BTreeMap::new();
        copy.insert("native".to_string(), serde_json::Value::Object(native_request.into_iter().collect()));
        let Ok(bytes) = crate::go_json::to_vec(&copy) else { continue };
        native.request = String::from_utf8_lossy(&bytes).into_owned();
    }
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let (ext, mut errs) = get_yeahmobi_ext(request);
        let params = EndpointTemplateParams {
            host: format!("gw-{}-bid.yeahtargeter.com", query_escape(&ext.zone_id)),
            ..Default::default()
        };
        let uri = match self.endpoint_template.resolve(&params) {
            Ok(u) => u,
            Err(e) => {
                errs.push(BidderError::other(e));
                return (vec![], errs);
            }
        };
        let mut req = request.clone();
        transform(&mut req);
        let body = match crate::go_json::to_vec(&req) {
            Ok(b) => b,
            Err(e) => {
                errs.push(BidderError::other(e.to_string()));
                return (vec![], errs);
            }
        };
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        let data = RequestData {
            method: "POST".into(),
            uri,
            body,
            headers,
            imp_ids: req.imp.iter().map(|i| i.id.clone()).collect(),
        };
        // Go appends the request only when `errs == nil`.
        if errs.is_empty() {
            (vec![data], errs)
        } else {
            (vec![], errs)
        }
    }

    fn make_bids(
        &self,
        request: &BidRequest,
        _request_data: &RequestData,
        response_data: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        let code = response_data.status_code;
        if code == 204 {
            return (None, vec![]);
        }
        if code == 400 {
            return (None, vec![BidderError::bad_input(format!("Unexpected status code: {code}."))]);
        }
        if code != 200 {
            return (None, vec![BidderError::bad_server_response(format!("Unexpected status code: {code}."))]);
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut out = BidderResponse::with_bids_capacity(1);
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let media_type = get_bid_type(&bid.impid, &request.imp);
                let mut video = ExtBidPrebidVideo::default();
                if bid.ext.is_some() {
                    // `var bidExt *yeahmobiBidExt` so a JSON `null` leaves it nil.
                    match unmarshal_raw::<Option<YeahmobiBidExt>>(&ext_bytes(&bid.ext)) {
                        Err(_) => return (None, vec![BidderError::other("bid.ext json unmarshal error")]),
                        Ok(Some(ext)) => {
                            if let Some(d) = ext.video.and_then(|v| v.duration) {
                                video.duration = d as i32;
                            }
                        }
                        Ok(None) => {}
                    }
                }
                let mut typed = TypedBid::new(bid, media_type);
                typed.bid_video = Some(video);
                out.bids.push(typed);
            }
        }
        (Some(out), vec![])
    }
}

fn get_bid_type(imp_id: &str, imps: &[Imp]) -> BidType {
    let mut bid_type = BidType::Banner;
    for imp in imps {
        if imp.id == imp_id {
            if imp.banner.is_some() {
                break;
            }
            if imp.video.is_some() {
                bid_type = BidType::Video;
                break;
            }
            if imp.native.is_some() {
                bid_type = BidType::Native;
                break;
            }
        }
    }
    bid_type
}
