//! Go `adapters/bidmatic/bidmatic.go`.
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

use crate::ortb::openrtb2::MarkupType;

pub struct Adapter {
    endpoint: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self { endpoint: endpoint.into() }
    }
}

/// Go `openrtb_ext.ExtImpBidmatic`. `source` is a `json.Number`, which jsonutil decodes from a
/// number or from a string.
#[derive(serde::Deserialize, Default)]
struct ExtImpBidmatic {
    #[serde(rename = "source", default)]
    source_id: Option<serde_json::Value>,
    #[serde(rename = "placementId", default)]
    placement_id: i64,
    #[serde(rename = "siteId", default)]
    site_id: i64,
    #[serde(rename = "bidFloor", default)]
    bid_floor: f64,
}

/// Go `json.Number` text (`""` when absent or null).
fn number_text(v: &Option<serde_json::Value>) -> String {
    match v {
        Some(serde_json::Value::Number(n)) => n.to_string(),
        Some(serde_json::Value::String(s)) => s.clone(),
        _ => String::new(),
    }
}

/// Go `json.isValidNumber`.
fn is_valid_number(s: &str) -> bool {
    let b = s.as_bytes();
    let mut i = 0;
    if i < b.len() && b[i] == b'-' {
        i += 1;
    }
    if i >= b.len() {
        return false;
    }
    if b[i] == b'0' {
        i += 1;
    } else if (b'1'..=b'9').contains(&b[i]) {
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
    } else {
        return false;
    }
    if i + 1 < b.len() && b[i] == b'.' && b[i + 1].is_ascii_digit() {
        i += 2;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
    }
    if i + 1 < b.len() && (b[i] == b'e' || b[i] == b'E') {
        i += 1;
        if b[i] == b'+' || b[i] == b'-' {
            i += 1;
        }
        if i >= b.len() {
            return false;
        }
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
    }
    i == b.len()
}

/// Go `strconv.ParseInt(s, 10, 64)` as `json.Number.Int64` reports it.
fn parse_int64(s: &str) -> Result<i64, BidderError> {
    s.parse::<i64>().map_err(|e| {
        use std::num::IntErrorKind::*;
        let reason = match e.kind() {
            PosOverflow | NegOverflow => "value out of range",
            _ => "invalid syntax",
        };
        BidderError::other(format!("strconv.ParseInt: parsing \"{s}\": {reason}"))
    })
}

/// Go `validateImpression`: rewrites `imp.ext` and `imp.bidfloor`, returns the source id.
fn validate_impression(imp: &mut Imp) -> Result<i64, BidderError> {
    if imp.ext.is_none() {
        return Err(BidderError::bad_input(format!(
            "ignoring imp id={}, extImpBidder is empty",
            imp.id
        )));
    }
    let imp_ext: ExtImpBidmatic = parse_imp_ext(
        imp,
        |e| BidderError::bad_input(format!("ignoring imp id={}, error while decoding extImpBidder, err: {e}", imp.id)),
        |e| BidderError::bad_input(format!("ignoring imp id={}, error while decoding impExt, err: {e}", imp.id)),
    )?;

    // common extension for all impressions
    let source = number_text(&imp_ext.source_id);
    // Go marshals an empty `json.Number` as 0 and any other invalid literal as an error.
    let source_json = if source.is_empty() { "0".to_string() } else { source.clone() };
    if !is_valid_number(&source_json) {
        return Err(BidderError::bad_input(format!(
            "ignoring imp id={}, error while marshaling impExt, err: json: invalid number literal \"{}\"",
            imp.id, source
        )));
    }
    let mut ext_json = format!("{{\"bidmatic\":{{\"source\":{source_json}");
    if imp_ext.placement_id != 0 {
        ext_json.push_str(&format!(",\"placementId\":{}", imp_ext.placement_id));
    }
    if imp_ext.site_id != 0 {
        ext_json.push_str(&format!(",\"siteId\":{}", imp_ext.site_id));
    }
    if imp_ext.bid_floor != 0.0 {
        ext_json.push_str(&format!(",\"bidFloor\":{}", serde_json::to_string(&imp_ext.bid_floor).unwrap_or_default()));
    }
    ext_json.push_str("}}");

    if imp_ext.bid_floor > 0.0 {
        imp.bidfloor = imp_ext.bid_floor;
    }
    imp.ext = Ext::from_slice(ext_json.as_bytes()).ok();

    parse_int64(&source)
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errors = vec![];
        // Go iterates a map (random order); keep first-seen order here.
        let mut groups: Vec<(i64, Vec<Imp>)> = vec![];
        for src in &request.imp {
            let mut imp = src.clone();
            let source_id = match validate_impression(&mut imp) {
                Ok(s) => s,
                Err(e) => {
                    errors.push(e);
                    continue;
                }
            };
            match groups.iter_mut().find(|(s, _)| *s == source_id) {
                Some((_, imps)) => imps.push(imp),
                None => groups.push((source_id, vec![imp])),
            }
        }
        if groups.is_empty() {
            return (vec![], errors);
        }

        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");

        let mut reqs = Vec::with_capacity(groups.len());
        for (source_id, imps) in groups {
            let mut req = request.clone();
            req.imp = imps;
            let body = match crate::go_json::to_vec(&req) {
                Ok(b) => b,
                Err(e) => {
                    errors.push(BidderError::other(format!("error while encoding bidRequest, err: {e}")));
                    return (vec![], errors);
                }
            };
            reqs.push(RequestData {
                method: "POST".into(),
                uri: format!("{}?source={}", self.endpoint, source_id),
                body,
                headers: headers.clone(),
                imp_ids: req.imp.iter().map(|i| i.id.clone()).collect(),
            });
        }
        (reqs, errors)
    }

    fn make_bids(
        &self,
        bid_req: &BidRequest,
        _unused: &RequestData,
        http_res: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if is_response_status_code_no_content(http_res) {
            return (None, vec![]);
        }
        if let Some(e) = check_response_status_code_for_errors(http_res) {
            return (None, vec![e]);
        }
        let unmarshaled: Result<BidResponse, BidderError> = if http_res.body.iter().all(|b| b" \t\r\n".contains(b)) {
            // Go (jsoniter) reads a NUL at EOF.
            Err(BidderError::FailedToUnmarshal("expect { or n, but found \u{0}".to_string()))
        } else {
            jsonutil::unmarshal(&http_res.body)
        };
        let bid_resp: BidResponse = match unmarshaled {
            Ok(r) => r,
            Err(e) => {
                return (
                    None,
                    vec![BidderError::bad_server_response(format!("error while decoding response, err: {e}"))],
                )
            }
        };

        let mut bid_response = BidderResponse::new();
        let mut errors = vec![];
        for sb in bid_resp.seatbid {
            for mut bid in sb.bid {
                let mut imp_ok = false;
                let mut media_type = BidType::Banner;
                bid.mtype = MarkupType::BANNER;
                for imp in &bid_req.imp {
                    if imp.id == bid.impid {
                        imp_ok = true;
                        if imp.video.is_some() {
                            media_type = BidType::Video;
                            bid.mtype = MarkupType::VIDEO;
                            break;
                        } else if imp.banner.is_some() {
                            media_type = BidType::Banner;
                            bid.mtype = MarkupType::BANNER;
                            break;
                        } else if imp.audio.is_some() {
                            media_type = BidType::Audio;
                            bid.mtype = MarkupType::AUDIO;
                            break;
                        } else if imp.native.is_some() {
                            media_type = BidType::Native;
                            bid.mtype = MarkupType::NATIVE;
                            break;
                        }
                    }
                }
                if !imp_ok {
                    errors.push(BidderError::bad_server_response(format!(
                        "ignoring bid id={}, request doesn't contain any impression with id={}",
                        bid.id, bid.impid
                    )));
                    continue;
                }
                bid_response.bids.push(TypedBid::new(bid, media_type));
            }
        }
        (Some(bid_response), errors)
    }
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
