//! Go `adapters/algorix/algorix.go`.
#![allow(dead_code, unused_imports)]

use crate::bid_types::BidType;
use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder, BidderResponse,
    ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{Bid, BidRequest, BidResponse, Imp};
use crate::ortb::Ext;
use crate::macros::{EndpointTemplate, EndpointTemplateParams};

use crate::ortb::openrtb2::Video;

pub struct Adapter {
    endpoint: EndpointTemplate,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl AsRef<str>) -> Result<Self, String> {
        let endpoint = parse_template(endpoint.as_ref())
            .map_err(|e| format!("unable to parse endpoint url template: {e}"))?;
        Ok(Self { endpoint })
    }
}

#[derive(serde::Deserialize, Default)]
struct ExtImpAlgorix {
    #[serde(default)]
    sid: String,
    #[serde(default)]
    token: String,
    #[serde(default)]
    region: String,
}

#[derive(serde::Deserialize, Default)]
struct ExtImpBidderPrebid {
    #[serde(default)]
    prebid: Option<ExtImpPrebid>,
}

#[derive(serde::Deserialize, Default)]
struct ExtImpPrebid {
    #[serde(default)]
    is_rewarded_inventory: Option<i8>,
}

#[derive(serde::Deserialize, Default)]
struct AlgorixResponseBidExt {
    #[serde(rename = "mediaType", default)]
    media_type: String,
}

fn get_region_info(region: &str) -> &'static str {
    match region {
        "APAC" => "apac.xyz",
        "USE" => "use.xyz",
        "EUC" => "euc.xyz",
        _ => "xyz",
    }
}

fn pre_process(request: &mut BidRequest) {
    for imp in request.imp.iter_mut() {
        if let Some(banner) = imp.banner.as_mut() {
            let missing = banner.w.map_or(true, |w| w == 0) || banner.h.map_or(true, |h| h == 0);
            if missing && !banner.format.is_empty() {
                let first = banner.format[0].clone();
                banner.w = Some(first.w);
                banner.h = Some(first.h);
            }
        }
        if imp.video.is_some() {
            let Ok(imp_ext) = unmarshal_ext::<ExtImpBidderPrebid>(imp.ext.as_ref()) else {
                continue;
            };
            if imp_ext.prebid.and_then(|p| p.is_rewarded_inventory) == Some(1) {
                if let Some(video) = imp.video.as_mut() {
                    video.ext = Ext::from_slice(br#"{"rewarded":1}"#).ok();
                }
            }
        }
    }
}

impl Adapter {
    fn make_request(&self, request: &BidRequest) -> Result<RequestData, BidderError> {
        // Go indexes `Imp[0]` and panics on an empty imp list; report it instead.
        let first = request
            .imp
            .first()
            .ok_or_else(|| BidderError::bad_input("Invalid ExtImpAlgoriX value"))?;
        let ext: ExtImpAlgorix = parse_imp_ext(first, |e| e, |e| e)
            .map_err(|_| BidderError::bad_input("Invalid ExtImpAlgoriX value"))?;

        let params = EndpointTemplateParams {
            source_id: path_escape(&ext.sid),
            account_id: path_escape(&ext.token),
            host: path_escape(get_region_info(&ext.region)),
            ..Default::default()
        };
        let end_point = self.endpoint.resolve(&params).map_err(BidderError::other)?;

        let mut req = request.clone();
        pre_process(&mut req);
        let body = crate::go_json::to_vec(&req).map_err(|e| BidderError::other(e.to_string()))?;

        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        headers.add("x-openrtb-version", "2.5");
        Ok(RequestData {
            method: "POST".into(),
            uri: end_point,
            body,
            headers,
            imp_ids: req.imp.iter().map(|i| i.id.clone()).collect(),
        })
    }
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        match self.make_request(request) {
            Ok(r) => (vec![r], vec![]),
            Err(e) => (vec![], vec![e]),
        }
    }

    fn make_bids(
        &self,
        internal_request: &BidRequest,
        _request_data: &RequestData,
        response_data: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if is_response_status_code_no_content(response_data) {
            return (None, vec![]);
        }
        if response_data.status_code == 400 {
            return (
                None,
                vec![BidderError::bad_input(format!(
                    "Unexpected status code: {}.",
                    response_data.status_code
                ))],
            );
        }
        if response_data.status_code != 200 {
            return (
                None,
                vec![BidderError::bad_server_response(format!(
                    "Unexpected status code: {}.",
                    response_data.status_code
                ))],
            );
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut bid_response = BidderResponse::with_bids_capacity(1);
        let mut errs = vec![];
        for seat_bid in bid_resp.seatbid {
            for bid in seat_bid.bid {
                match get_bid_type(&bid, &internal_request.imp) {
                    Ok(t) => bid_response.bids.push(TypedBid::new(bid, t)),
                    Err(e) => errs.push(e),
                }
            }
        }
        (Some(bid_response), errs)
    }
}

fn get_bid_type(bid: &Bid, imps: &[Imp]) -> Result<BidType, BidderError> {
    if let Ok(ext) = unmarshal_ext::<AlgorixResponseBidExt>(bid.ext.as_ref()) {
        match ext.media_type.as_str() {
            "banner" => return Ok(BidType::Banner),
            "native" => return Ok(BidType::Native),
            "video" => return Ok(BidType::Video),
            _ => {}
        }
    }
    let mut media_type = BidType::default();
    let mut cnt = 0;
    for imp in imps {
        if imp.id == bid.impid {
            if imp.banner.is_some() {
                cnt += 1;
                media_type = BidType::Banner;
            }
            if imp.native.is_some() {
                cnt += 1;
                media_type = BidType::Native;
            }
            if imp.video.is_some() {
                cnt += 1;
                media_type = BidType::Video;
            }
        }
    }
    if cnt == 1 {
        return Ok(media_type);
    }
    Err(BidderError::other(format!("unable to fetch mediaType in multi-format: {}", bid.impid)))
}

// ── local helpers (Go `adapters.ExtImpBidder`, `openrtb_ext.ExtBid`) ─────────────────────────

/// Go `adapters.ExtImpBidder` (only `bidder` is used here).
#[derive(serde::Deserialize, Default)]
struct ExtImpBidder {
    #[serde(default)]
    bidder: Option<Ext>,
}

/// Go `jsonutil.Unmarshal(raw, &v)` on a `json.RawMessage` field. An absent message is empty
/// input, which json-iterator rejects with the same `expect { or n, but found` text.
fn unmarshal_ext<T: serde::de::DeserializeOwned>(ext: Option<&Ext>) -> Result<T, BidderError> {
    match ext {
        Some(e) => jsonutil::unmarshal(e.to_json().as_bytes()),
        None => Err(BidderError::FailedToUnmarshal("expect { or n, but found \u{0}".to_string())),
    }
}

/// Go: unmarshal `imp.ext` into `ExtImpBidder`, then `.Bidder` into `T`; the two errors are
/// returned separately so adapters can wrap them differently.
fn parse_imp_ext<T: serde::de::DeserializeOwned>(
    imp: &Imp,
    wrap_bidder_ext: impl Fn(BidderError) -> BidderError,
    wrap_params: impl Fn(BidderError) -> BidderError,
) -> Result<T, BidderError> {
    let bidder_ext: ExtImpBidder = unmarshal_ext(imp.ext.as_ref()).map_err(wrap_bidder_ext)?;
    unmarshal_ext(bidder_ext.bidder.as_ref()).map_err(wrap_params)
}

/// Go `openrtb_ext.ExtBid` (`bid.ext.prebid.type`).
#[derive(serde::Deserialize, Default)]
struct ExtBid {
    #[serde(default)]
    prebid: Option<ExtBidPrebid>,
}

#[derive(serde::Deserialize, Default)]
struct ExtBidPrebid {
    #[serde(rename = "type", default)]
    bid_type: String,
}

/// Go `jsonutil.Unmarshal(bid.Ext, &ExtBid)`: `Err` on a malformed ext.
fn parse_bid_ext(bid: &Bid) -> Option<Result<ExtBid, BidderError>> {
    bid.ext.as_ref().map(|e| jsonutil::unmarshal(e.to_json().as_bytes()))
}

/// Go `strconv.FormatFloat(price, 'f', -1, 64)`.
fn format_price(price: f64) -> String {
    format!("{price}")
}

/// Go `template.New("endpointTemplate").Parse(endpoint)`. `{{Malformed}}` is a Go parse error
/// (unknown function), which `EndpointTemplate::parse` only reports at resolve time, so a dry
/// run with empty params catches it here.
fn parse_template(endpoint: &str) -> Result<EndpointTemplate, String> {
    let t = EndpointTemplate::parse(endpoint)?;
    t.resolve(&EndpointTemplateParams::default())?;
    Ok(t)
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

/// Go `url.PathEscape`.
fn path_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'$' | b'&' | b'+'
            | b'=' | b':' | b'@' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
