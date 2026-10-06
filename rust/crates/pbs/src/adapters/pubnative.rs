//! Go `adapters/pubnative/pubnative.go`.

use serde::Deserialize;

use crate::bid_types::BidType;
use crate::bidder::{Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{Banner, BidRequest, BidResponse, Imp};
use crate::ortb::Ext;

pub struct Adapter {
    uri: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self { uri: endpoint.into() }
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpBidder {
    bidder: Option<Ext>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpPubnative {
    zone_id: i64,
    app_auth_token: String,
}

fn ext_text(ext: &Option<Ext>) -> Vec<u8> {
    ext.as_ref().map(|e| e.to_json().into_bytes()).unwrap_or_default()
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

fn check_request(request: &BidRequest) -> Result<(), BidderError> {
    match &request.device {
        Some(d) if !d.os.is_empty() => Ok(()),
        _ => Err(BidderError::bad_input("Impression is missing device OS information")),
    }
}

fn convert_impression(imp: &mut Imp) -> Result<(), BidderError> {
    if imp.banner.is_none() && imp.video.is_none() && imp.native.is_none() {
        return Err(BidderError::bad_input("Pubnative only supports banner, video or native ads."));
    }
    if let Some(banner) = imp.banner.as_ref() {
        imp.banner = Some(convert_banner(banner)?);
    }
    Ok(())
}

/// Make sure that banner has openrtb 2.3-compatible size information.
fn convert_banner(banner: &Banner) -> Result<Banner, BidderError> {
    let missing = match (banner.w, banner.h) {
        (Some(w), Some(h)) => w == 0 || h == 0,
        _ => true,
    };
    if missing {
        if let Some(f) = banner.format.first() {
            let mut copy = banner.clone();
            copy.w = Some(f.w);
            copy.h = Some(f.h);
            return Ok(copy);
        }
        return Err(BidderError::bad_input("Size information missing for banner"));
    }
    Ok(banner.clone())
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut request_data = Vec::with_capacity(request.imp.len());
        let mut errs = vec![];
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");

        if let Err(e) = check_request(request) {
            return (vec![], vec![e]);
        }

        for imp in &request.imp {
            let bidder_ext: ExtImpBidder = match jsonutil::unmarshal(&ext_text(&imp.ext)) {
                Ok(e) => e,
                Err(e) => {
                    errs.push(e);
                    continue;
                }
            };
            let ext: ExtImpPubnative = match jsonutil::unmarshal(&ext_text(&bidder_ext.bidder)) {
                Ok(e) => e,
                Err(e) => {
                    errs.push(e);
                    continue;
                }
            };
            let mut imp = imp.clone();
            if let Err(e) = convert_impression(&mut imp) {
                errs.push(e);
                continue;
            }
            let mut request_copy = request.clone();
            request_copy.imp = vec![imp];
            let req_json = match crate::go_json::to_vec(&request_copy) {
                Ok(b) => b,
                Err(e) => return (vec![], vec![BidderError::other(e.to_string())]),
            };
            // `url.Values.Encode` sorts by key: apptoken, zoneid.
            let query = format!(
                "apptoken={}&zoneid={}",
                query_escape(&ext.app_auth_token),
                query_escape(&ext.zone_id.to_string())
            );
            request_data.push(RequestData {
                method: "POST".into(),
                uri: format!("{}?{}", self.uri, query),
                body: req_json,
                headers: headers.clone(),
                imp_ids: request_copy.imp.iter().map(|i| i.id.clone()).collect(),
            });
        }
        (request_data, errs)
    }

    fn make_bids(
        &self,
        request: &BidRequest,
        _request_data: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if response.status_code == 204 {
            return (None, vec![]);
        }
        let msg = || {
            format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info",
                response.status_code
            )
        };
        if response.status_code == 400 {
            return (None, vec![BidderError::bad_input(msg())]);
        }
        if response.status_code != 200 {
            return (None, vec![BidderError::bad_server_response(msg())]);
        }
        let parsed: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![BidderError::bad_server_response(e.to_string())]),
        };
        let mut bid_response = BidderResponse::with_bids_capacity(1);
        for sb in parsed.seatbid {
            for bid in sb.bid {
                if bid.price != 0.0 {
                    let t = get_media_type_for_imp(&bid.impid, &request.imp);
                    bid_response.bids.push(TypedBid::new(bid, t));
                }
            }
        }
        (Some(bid_response), vec![])
    }
}

fn get_media_type_for_imp(imp_id: &str, imps: &[Imp]) -> BidType {
    let mut media_type = BidType::Banner;
    for imp in imps {
        if imp.id == imp_id {
            if imp.video.is_some() {
                media_type = BidType::Video;
            } else if imp.native.is_some() {
                media_type = BidType::Native;
            }
            return media_type;
        }
    }
    media_type
}
