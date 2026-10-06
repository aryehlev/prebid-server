//! Go `adapters/gamma/gamma.go`.
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
    uri: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self { uri: endpoint.into() }
    }
}

#[derive(serde::Deserialize, Default)]
struct ExtImpGamma {
    #[serde(rename = "id", default)]
    partner_id: String,
    #[serde(rename = "zid", default)]
    zone_id: String,
    #[serde(rename = "wid", default)]
    web_id: String,
}

/// Go `gammaBid`: an `openrtb2.Bid` plus the vast fields.
#[derive(serde::Deserialize, Default)]
struct GammaBid {
    #[serde(flatten)]
    bid: Bid,
    #[serde(rename = "vastXml", default)]
    vast_xml: String,
    #[serde(rename = "vastUrl", default)]
    vast_url: String,
}

#[derive(serde::Deserialize, Default)]
struct GammaSeatBid {
    #[serde(default)]
    bid: Vec<GammaBid>,
}

#[derive(serde::Deserialize, Default)]
struct GammaBidResponse {
    #[serde(default)]
    id: String,
    #[serde(default)]
    seatbid: Vec<GammaSeatBid>,
}

fn check_params(ext: &ExtImpGamma) -> Result<(), BidderError> {
    if ext.partner_id.is_empty() {
        return Err(BidderError::bad_input("PartnerID is empty"));
    }
    if ext.zone_id.is_empty() {
        return Err(BidderError::bad_input("ZoneID is empty"));
    }
    if ext.web_id.is_empty() {
        return Err(BidderError::bad_input("WebID is empty"));
    }
    Ok(())
}

fn add_header_if_non_empty(headers: &mut Header, name: &str, value: &str) {
    if !value.is_empty() {
        headers.add(name, value);
    }
}

impl Adapter {
    fn make_request(&self, request: &BidRequest, imp: &Imp) -> Result<RequestData, BidderError> {
        let bidder_ext: ExtImpBidder = unmarshal_ext(imp.ext.as_ref())
            .map_err(|_| BidderError::bad_input("ext.bidder not provided"))?;
        let gamma_ext: ExtImpGamma = unmarshal_ext(bidder_ext.bidder.as_ref())
            .map_err(|_| BidderError::bad_input("ext.bidder.publisher not provided"))?;
        check_params(&gamma_ext)?;

        let mut uri = self.uri.clone();
        uri.push_str(&format!("?id={}", gamma_ext.partner_id));
        uri.push_str(&format!("&zid={}", gamma_ext.zone_id));
        uri.push_str(&format!("&wid={}", gamma_ext.web_id));
        uri.push_str(&format!("&bidid={}", imp.id));
        uri.push_str("&hb=pbmobile");
        if let Some(d) = &request.device {
            if !d.ip.is_empty() {
                uri.push_str(&format!("&device_ip={}", d.ip));
            }
            if !d.model.is_empty() {
                uri.push_str(&format!("&device_model={}", d.model));
            }
            if !d.os.is_empty() {
                uri.push_str(&format!("&device_os={}", d.os));
            }
            if !d.ua.is_empty() {
                uri.push_str(&format!("&device_ua={}", query_escape(&d.ua)));
            }
            if !d.ifa.is_empty() {
                uri.push_str(&format!("&device_ifa={}", d.ifa));
            }
        }
        if let Some(app) = &request.app {
            if !app.id.is_empty() {
                uri.push_str(&format!("&app_id={}", app.id));
            }
            if !app.bundle.is_empty() {
                uri.push_str(&format!("&app_bundle={}", app.bundle));
            }
            if !app.name.is_empty() {
                uri.push_str(&format!("&app_name={}", app.name));
            }
        }
        let mut headers = Header::new();
        headers.add("Accept", "*/*");
        headers.add("x-openrtb-version", "2.5");
        if let Some(d) = &request.device {
            add_header_if_non_empty(&mut headers, "User-Agent", &d.ua);
            add_header_if_non_empty(&mut headers, "X-Forwarded-For", &d.ip);
            add_header_if_non_empty(&mut headers, "Accept-Language", &d.language);
            if let Some(dnt) = d.dnt {
                add_header_if_non_empty(&mut headers, "DNT", &dnt.to_string());
            }
        }
        headers.add("Connection", "keep-alive");
        headers.add("cache-control", "no-cache");
        headers.add("Accept-Encoding", "gzip, deflate");

        Ok(RequestData {
            method: "GET".into(),
            uri,
            body: vec![],
            headers,
            imp_ids: vec![imp.id.clone()],
        })
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
            return (vec![], vec![BidderError::bad_input("No impressions in the bid request")]);
        }
        let mut invalid = vec![false; request.imp.len()];
        for (i, imp) in request.imp.iter().enumerate() {
            if imp.banner.is_some() {
                // Go fills banner w/h from the first format on a copy; the GET request never
                // carries the banner, so nothing observable remains.
            } else if imp.video.is_none() {
                errs.push(BidderError::bad_input(format!(
                    "Gamma only supports banner and video media types. Ignoring imp id={}",
                    imp.id
                )));
                invalid[i] = true;
            }
        }
        let invalid_count = invalid.iter().filter(|b| **b).count();
        if invalid_count == request.imp.len() {
            // only true if every Imp was not a Banner or a Video
            errs.push(BidderError::bad_input("No valid impression in the bid request"));
            return (vec![], errs);
        }
        let mut reqs = vec![];
        for (i, imp) in request.imp.iter().enumerate() {
            if invalid[i] {
                continue;
            }
            match self.make_request(request, imp) {
                Ok(r) => reqs.push(r),
                Err(e) => errs.push(e),
            }
        }
        (reqs, errs)
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
        if response_data.status_code != 200 {
            return (
                None,
                vec![BidderError::bad_server_response(format!(
                    "Unexpected status code: {}. Run with request.debug = 1 for more info",
                    response_data.status_code
                ))],
            );
        }
        let unmarshaled: Result<GammaBidResponse, BidderError> =
            if response_data.body.iter().all(|b| b" \t\r\n".contains(b)) {
                Err(BidderError::FailedToUnmarshal("expect { or n, but found \u{0}".to_string()))
            } else {
                jsonutil::unmarshal(&response_data.body)
            };
        let gamma_resp = match unmarshaled {
            Ok(r) => r,
            // Go formats the error pointer with `%d`, which prints the struct with a bad verb.
            Err(e) => {
                return (
                    None,
                    vec![BidderError::bad_server_response(format!(
                        "bad server response: &{{%!d(string={e})}}. "
                    ))],
                )
            }
        };

        // (Section 7.1 No-Bid Signaling)
        if gamma_resp.seatbid.is_empty() {
            return (None, vec![]);
        }
        let mut bid_response = BidderResponse::with_bids_capacity(gamma_resp.seatbid[0].bid.len());
        let mut errs = vec![];
        let media_type = get_media_type_for_imp(&gamma_resp.id, &internal_request.imp);
        for sb in gamma_resp.seatbid {
            for g_bid in sb.bid {
                match convert_bid(g_bid, media_type) {
                    Some(bid) => bid_response.bids.push(TypedBid::new(bid, media_type)),
                    None => errs.push(BidderError::bad_server_response(
                        "Missing Ad Markup. Run with request.debug = 1 for more info",
                    )),
                }
            }
        }
        (Some(bid_response), errs)
    }
}

fn convert_bid(g_bid: GammaBid, media_type: BidType) -> Option<Bid> {
    let mut bid = g_bid.bid;
    if media_type == BidType::Video {
        // Return inline VAST XML Document (Section 6.4.2)
        if !g_bid.vast_xml.is_empty() {
            if !g_bid.vast_url.is_empty() {
                bid.nurl = g_bid.vast_url;
            }
            bid.adm = g_bid.vast_xml;
        } else {
            return None;
        }
    } else if bid.adm.is_empty() {
        return None;
    }
    Some(bid)
}

/// Go `getMediaTypeForImp`.
fn get_media_type_for_imp(imp_id: &str, imps: &[Imp]) -> BidType {
    for imp in imps {
        if imp.id == imp_id {
            if imp.video.is_some() {
                return BidType::Video;
            }
            return BidType::Banner;
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
