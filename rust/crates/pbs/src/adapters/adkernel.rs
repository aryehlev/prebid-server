//! Go `adapters/adkernel/adkernel.go`.

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

/// `Unmarshal(imp.Ext, &bidderExt)` then `Unmarshal(bidderExt.Bidder, &params)`.
fn parse_bidder_ext<T: serde::de::DeserializeOwned>(imp: &Imp) -> Result<T, BidderError> {
    let outer: ExtImpBidder = jsonutil::unmarshal(&ext_bytes(&imp.ext))?;
    jsonutil::unmarshal(&ext_bytes(&outer.bidder))
}

fn json_headers() -> Header {
    let mut headers = Header::new();
    headers.add("Content-Type", "application/json;charset=utf-8");
    headers.add("Accept", "application/json");
    headers
}

const MF_SUFFIX: &str = "__mf";
const MF_SUFFIX_BANNER: &str = "b__mf";
const MF_SUFFIX_VIDEO: &str = "v__mf";
const MF_SUFFIX_AUDIO: &str = "a__mf";
const MF_SUFFIX_NATIVE: &str = "n__mf";

#[derive(Deserialize, Default, Clone, PartialEq, Eq)]
#[serde(default)]
struct ExtImpAdkernel {
    #[serde(rename = "zoneId")]
    zone_id: i64,
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

    fn build_adapter_request(
        &self,
        request: &BidRequest,
        params: &ExtImpAdkernel,
        imps: Vec<Imp>,
    ) -> Result<RequestData, BidderError> {
        let imp_ids = imps.iter().map(|i| i.id.clone()).collect();
        let new_request = create_bid_request(request, imps);
        let body = crate::go_json::to_vec(&new_request).map_err(|e| BidderError::other(e.to_string()))?;
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        headers.add("x-openrtb-version", "2.5");
        let url = self
            .endpoint_template
            .resolve(&EndpointTemplateParams { zone_id: params.zone_id.to_string(), ..Default::default() })
            .map_err(BidderError::other)?;
        Ok(RequestData { method: "POST".into(), uri: url, body, headers, imp_ids })
    }
}

fn get_impression_ext(imp: &Imp) -> Result<ExtImpAdkernel, BidderError> {
    let mut outer: Option<ExtImpBidder> = None;
    let e = jsonutil::unmarshal::<ExtImpBidder>(&ext_bytes(&imp.ext));
    match e {
        Ok(o) => outer = Some(o),
        Err(e) => {
            // Go's nil `imp.Ext` reports the NUL byte; keep that wording.
            let msg = if ext_bytes(&imp.ext).is_empty() { "expect { or n, but found \u{0}".to_string() } else { e.to_string() };
            return Err(BidderError::bad_input(msg));
        }
    }
    let bidder = ext_bytes(&outer.unwrap().bidder);
    if bidder.is_empty() {
        return Err(BidderError::bad_input("expect { or n, but found \u{0}"));
    }
    jsonutil::unmarshal(&bidder).map_err(|e| BidderError::bad_input(e.to_string()))
}

fn is_multi_format_imp(imp: &Imp) -> bool {
    let count = imp.video.is_some() as u8 + imp.audio.is_some() as u8 + imp.banner.is_some() as u8 + imp.native.is_some() as u8;
    count > 1
}

fn split_multi_format_imp(imp: &Imp) -> Vec<Imp> {
    let mut out = Vec::with_capacity(4);
    if imp.banner.is_some() {
        let mut c = imp.clone();
        c.video = None;
        c.native = None;
        c.audio = None;
        c.id.push_str(MF_SUFFIX_BANNER);
        out.push(c);
    }
    if imp.video.is_some() {
        let mut c = imp.clone();
        c.banner = None;
        c.native = None;
        c.audio = None;
        c.id.push_str(MF_SUFFIX_VIDEO);
        out.push(c);
    }
    if imp.native.is_some() {
        let mut c = imp.clone();
        c.banner = None;
        c.video = None;
        c.audio = None;
        c.id.push_str(MF_SUFFIX_NATIVE);
        out.push(c);
    }
    if imp.audio.is_some() {
        let mut c = imp.clone();
        c.banner = None;
        c.video = None;
        c.native = None;
        c.id.push_str(MF_SUFFIX_AUDIO);
        out.push(c);
    }
    out
}

fn create_bid_request(prebid: &BidRequest, imps: Vec<Imp>) -> BidRequest {
    let mut r = prebid.clone();
    r.imp = imps;
    if let Some(site) = r.site.as_mut() {
        site.publisher = None;
    }
    if let Some(app) = r.app.as_mut() {
        app.publisher = None;
    }
    r
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::bad_input("No impression in the bid request")]);
        }
        // getImpressionsInfo
        let mut errs = Vec::new();
        let mut imps = Vec::new();
        let mut exts = Vec::new();
        for imp in &request.imp {
            let ext = match get_impression_ext(imp) {
                Ok(e) => e,
                Err(e) => {
                    errs.push(e);
                    continue;
                }
            };
            if ext.zone_id < 1 {
                errs.push(BidderError::bad_input(format!(
                    "Invalid zoneId value: {}. Ignoring imp id={}",
                    ext.zone_id, imp.id
                )));
                continue;
            }
            imps.push(imp.clone());
            exts.push(ext);
        }
        if imps.is_empty() {
            return (vec![], errs);
        }

        // dispatchImpressions: group by zoneId (Go iterates a map; first-seen order here).
        let mut groups: Vec<(ExtImpAdkernel, Vec<Imp>)> = Vec::new();
        for (mut imp, ext) in imps.into_iter().zip(exts) {
            imp.ext = None;
            let idx = match groups.iter().position(|(k, _)| *k == ext) {
                Some(i) => i,
                None => {
                    groups.push((ext, Vec::new()));
                    groups.len() - 1
                }
            };
            if is_multi_format_imp(&imp) {
                groups[idx].1.extend(split_multi_format_imp(&imp));
            } else {
                groups[idx].1.push(imp);
            }
        }

        let mut result = Vec::with_capacity(groups.len());
        for (params, imps) in groups {
            match self.build_adapter_request(request, &params, imps) {
                Ok(r) => result.push(r),
                Err(e) => errs.push(e),
            }
        }
        (result, errs)
    }

    fn make_bids(
        &self,
        _internal_request: &BidRequest,
        _external_request: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if response.status_code == 204 {
            return (None, vec![]);
        }
        if response.status_code != 200 {
            return (
                None,
                vec![BidderError::bad_server_response(format!(
                    "Unexpected http status code: {}",
                    response.status_code
                ))],
            );
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            // Go formats the error with `%d`, which yields `%!d(...)` noise; the message here is the plain error.
            Err(e) => return (None, vec![BidderError::bad_server_response(format!("Bad server response: {e}"))]),
        };
        if bid_resp.seatbid.len() != 1 {
            return (
                None,
                vec![BidderError::bad_server_response(format!(
                    "Invalid SeatBids count: {}",
                    bid_resp.seatbid.len()
                ))],
            );
        }
        let cur = bid_resp.cur.clone();
        let seat_bid = bid_resp.seatbid.into_iter().next().unwrap();
        let mut out = BidderResponse::with_bids_capacity(seat_bid.bid.len());
        out.currency = cur;
        for mut bid in seat_bid.bid {
            if bid.impid.ends_with(MF_SUFFIX) {
                // Go slices off `len(suffix)+1` bytes (the type letter too).
                let start = bid.impid.len() - MF_SUFFIX.len() - 1;
                bid.impid.truncate(start);
            }
            let t = match bid.mtype {
                MarkupType::BANNER => BidType::Banner,
                MarkupType::AUDIO => BidType::Audio,
                MarkupType::NATIVE => BidType::Native,
                MarkupType::VIDEO => BidType::Video,
                m => return (None, vec![BidderError::bad_server_response(format!("Unsupported MType {}", m.0))]),
            };
            out.bids.push(TypedBid::new(bid, t));
        }
        (Some(out), vec![])
    }
}
