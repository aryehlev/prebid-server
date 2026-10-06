//! Go `adapters/advangelists/advangelists.go`.
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

/// Go `openrtb_ext.ExtImpAdvangelists` (also the grouping key).
#[derive(serde::Deserialize, Default, Clone, PartialEq, Eq)]
struct ExtImpAdvangelists {
    #[serde(rename = "pubid", default)]
    publisher_id: String,
    #[serde(default)]
    placement: String,
}

fn get_impression_ext(imp: &Imp) -> Result<ExtImpAdvangelists, BidderError> {
    parse_imp_ext(
        imp,
        |e| BidderError::bad_input(e.to_string()),
        |e| BidderError::bad_input(e.to_string()),
    )
}

/// Go `getImpressionsInfo`.
fn get_impressions_info(imps: &[Imp]) -> (Vec<Imp>, Vec<ExtImpAdvangelists>, Vec<BidderError>) {
    let mut errors = vec![];
    let mut res_imps = vec![];
    let mut res_exts = vec![];
    for imp in imps {
        let ext = match get_impression_ext(imp) {
            Ok(e) => e,
            Err(e) => {
                errors.push(e);
                continue;
            }
        };
        if ext.publisher_id.is_empty() {
            errors.push(BidderError::bad_input("No pubid value provided"));
            continue;
        }
        res_imps.push(imp.clone());
        res_exts.push(ext);
    }
    (res_imps, res_exts, errors)
}

/// Go `compatImpression`: do not forward ext, make banner w/h explicit.
fn compat_impression(imp: &mut Imp) -> Result<(), BidderError> {
    imp.ext = None;
    if let Some(banner) = imp.banner.as_mut() {
        // As banner.w/h are required fields for the platform, take the first format entry
        if banner.w.is_none() || banner.h.is_none() {
            if banner.format.is_empty() {
                return Err(BidderError::bad_input("Expected at least one banner.format entry or explicit w/h"));
            }
            let format = banner.format.remove(0);
            banner.w = Some(format.w);
            banner.h = Some(format.h);
        }
    }
    Ok(())
}

fn create_bid_request(prebid: &BidRequest, params: &ExtImpAdvangelists, imps: Vec<Imp>) -> BidRequest {
    let mut req = prebid.clone();
    req.imp = imps;
    for imp in req.imp.iter_mut() {
        imp.tagid = params.placement.clone();
    }
    if let Some(site) = req.site.as_mut() {
        site.publisher = None;
        site.domain = String::new();
    }
    if let Some(app) = req.app.as_mut() {
        app.publisher = None;
    }
    req
}

impl Adapter {
    fn build_adapter_request(
        &self,
        prebid: &BidRequest,
        params: &ExtImpAdvangelists,
        imps: Vec<Imp>,
    ) -> Result<RequestData, BidderError> {
        let imp_ids: Vec<String> = imps.iter().map(|i| i.id.clone()).collect();
        let new_req = create_bid_request(prebid, params, imps);
        let body = crate::go_json::to_vec(&new_req).map_err(|e| BidderError::other(e.to_string()))?;
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        headers.add("x-openrtb-version", "2.5");
        let url = self
            .endpoint
            .resolve(&EndpointTemplateParams { publisher_id: params.publisher_id.clone(), ..Default::default() })
            .map_err(BidderError::other)?;
        Ok(RequestData { method: "POST".into(), uri: url, body, headers, imp_ids })
    }
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = vec![];
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::bad_input("No impression in the bid request")]);
        }
        let (imps, imp_exts, err) = get_impressions_info(&request.imp);
        if imps.is_empty() {
            return (vec![], err);
        }
        errs.extend(err);

        // Go groups into a map (random iteration order); first-seen order is kept here.
        let mut groups: Vec<(ExtImpAdvangelists, Vec<Imp>)> = vec![];
        for (mut imp, ext) in imps.into_iter().zip(imp_exts) {
            if let Err(e) = compat_impression(&mut imp) {
                errs.push(e);
                continue;
            }
            match groups.iter_mut().find(|(k, _)| *k == ext) {
                Some((_, v)) => v.push(imp),
                None => groups.push((ext, vec![imp])),
            }
        }
        if groups.is_empty() {
            return (vec![], errs);
        }
        let mut result = Vec::with_capacity(groups.len());
        for (k, imps) in groups {
            match self.build_adapter_request(request, &k, imps) {
                Ok(r) => result.push(r),
                Err(e) => {
                    errs.push(e);
                    return (vec![], errs);
                }
            }
        }
        (result, errs)
    }

    fn make_bids(
        &self,
        internal_request: &BidRequest,
        _external_request: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if is_response_status_code_no_content(response) {
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
            // Go formats the error pointer with `%d`, which prints the struct with a bad verb.
            Err(e) => {
                return (
                    None,
                    vec![BidderError::bad_server_response(format!("Bad server response: &{{%!d(string={e})}}"))],
                )
            }
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
        let seat_bid = bid_resp.seatbid.into_iter().next().unwrap_or_default();
        let mut bid_response = BidderResponse::with_bids_capacity(seat_bid.bid.len());
        for bid in seat_bid.bid {
            let t = get_media_type_for_imp_id(&bid.impid, &internal_request.imp);
            bid_response.bids.push(TypedBid::new(bid, t));
        }
        (Some(bid_response), vec![])
    }
}

fn get_media_type_for_imp_id(imp_id: &str, imps: &[Imp]) -> BidType {
    for imp in imps {
        if imp.id == imp_id && imp.video.is_some() {
            return BidType::Video;
        }
    }
    BidType::Banner
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
