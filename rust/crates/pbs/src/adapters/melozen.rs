//! Go `adapters/melozen/melozen.go`.
#![allow(dead_code, unused_imports)]

use crate::bid_types::BidType;
use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder, BidderResponse,
    ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::macros::{EndpointTemplate, EndpointTemplateParams};
use crate::ortb::openrtb2::{Bid, BidRequest, BidResponse, Imp};
use crate::ortb::Ext;
use sonic_rs::{JsonContainerTrait, JsonValueTrait};
pub struct Adapter {
    endpoint_template: EndpointTemplate,
}

impl Adapter {
    /// Go `Builder`: fails when the endpoint is not a valid template.
    pub fn new(endpoint: &str) -> Result<Self, String> {
        Ok(Self { endpoint_template: parse_template(endpoint)? })
    }
}

/// Go `splitImpressionsByMediaType`.
fn split_impressions_by_media_type(imp: &Imp) -> Result<Vec<Imp>, BidderError> {
    if imp.banner.is_none() && imp.native.is_none() && imp.video.is_none() {
        return Err(BidderError::bad_input("Invalid MediaType. MeloZen only supports Banner, Video and Native."));
    }
    let mut imps = Vec::with_capacity(2);
    if imp.banner.is_some() {
        let mut c = imp.clone();
        c.video = None;
        c.native = None;
        imps.push(c);
    }
    if imp.video.is_some() {
        let mut c = imp.clone();
        c.banner = None;
        c.native = None;
        imps.push(c);
    }
    if imp.native.is_some() {
        let mut c = imp.clone();
        c.banner = None;
        c.video = None;
        imps.push(c);
    }
    Ok(imps)
}

fn media_type_for_bid(bid: &Bid) -> Result<BidType, BidderError> {
    if bid.ext.is_some() {
        if let Some(t) = prebid_type(bid.ext.as_ref()) {
            return parse_bid_type(&t);
        }
    }
    Err(BidderError::bad_server_response(format!(
        "Failed to parse bid mediatype for impression \"{}\"",
        bid.impid
    )))
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();
        let mut errors = Vec::new();
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");

        for imp in &request.imp {
            let params = match imp_bidder_params(imp) {
                Ok(p) => p,
                Err((_, e)) => {
                    errors.push(e);
                    continue;
                }
            };
            let pub_id = match str_field(params, "pubId", "openrtb_ext.ImpExtMeloZen.PubId") {
                Ok(v) => v,
                Err(e) => {
                    errors.push(e);
                    continue;
                }
            };
            let url = match self
                .endpoint_template
                .resolve(&EndpointTemplateParams { publisher_id: pub_id, ..Default::default() })
            {
                Ok(u) => u,
                Err(e) => {
                    errors.push(BidderError::other(e));
                    continue;
                }
            };

            let mut imp = imp.clone();
            // Convert Floor into USD
            if imp.bidfloor > 0.0 && !imp.bidfloorcur.is_empty() && !imp.bidfloorcur.eq_ignore_ascii_case("USD") {
                match req_info.convert_currency(imp.bidfloor, &imp.bidfloorcur, "USD") {
                    Ok(v) => {
                        imp.bidfloorcur = "USD".into();
                        imp.bidfloor = v;
                    }
                    Err(e) => {
                        errors.push(e);
                        continue;
                    }
                }
            }

            let split = match split_impressions_by_media_type(&imp) {
                Ok(s) => s,
                Err(e) => {
                    errors.push(e);
                    continue;
                }
            };
            for impression in split {
                let mut req = request.clone();
                req.imp = vec![impression];
                match crate::go_json::to_vec(&req) {
                    Ok(body) => requests.push(RequestData {
                        method: "POST".into(),
                        uri: url.clone(),
                        body,
                        headers: headers.clone(),
                        imp_ids: req.imp.iter().map(|i| i.id.clone()).collect(),
                    }),
                    Err(e) => errors.push(BidderError::other(e.to_string())),
                }
            }
        }
        (requests, errors)
    }

    fn make_bids(
        &self,
        _request: &BidRequest,
        request_data: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if is_response_status_code_no_content(response) {
            return (None, vec![]);
        }
        if let Some(err) = check_response_status_code_for_errors(response) {
            return (None, vec![err]);
        }
        // Go parses the outgoing request body (`bidReq`) and never uses it; the error still counts.
        if let Err(e) = jsonutil::unmarshal::<BidRequest>(&request_data.body) {
            return (None, vec![e]);
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };

        let mut bidder_response = BidderResponse::new();
        let mut errors = Vec::new();
        for seat_bid in bid_resp.seatbid {
            for bid in seat_bid.bid {
                match media_type_for_bid(&bid) {
                    Ok(t) => bidder_response.bids.push(TypedBid::new(bid, t)),
                    Err(e) => errors.push(e),
                }
            }
        }
        (Some(bidder_response), errors)
    }
}

// ---- local helpers (Go `jsonutil.Unmarshal` into `json.RawMessage`-backed params) ----------
type Val = sonic_rs::Value;

fn unmarshal_err(first: char) -> BidderError {
    BidderError::FailedToUnmarshal(format!("expect {{ or n, but found {first}"))
}

/// Go `jsonutil.Unmarshal(raw, &struct)` first-byte check: absent input fails on the NUL byte,
/// anything but an object or `null` fails on its first character. `Ok(None)` is `null`.
fn obj_of(v: Option<&Val>) -> Result<Option<&Val>, BidderError> {
    let Some(v) = v else {
        return Err(unmarshal_err('\0'));
    };
    if v.is_object() {
        Ok(Some(v))
    } else if v.is_null() {
        Ok(None)
    } else {
        Err(unmarshal_err(sonic_rs::to_string(v).ok().and_then(|s| s.chars().next()).unwrap_or('\0')))
    }
}

/// jsoniter matches struct keys case-insensitively.
fn field<'a>(obj: Option<&'a Val>, name: &str) -> Option<&'a Val> {
    let obj = obj?.as_object()?;
    obj.iter().find(|(k, _)| *k == name).or_else(|| obj.iter().find(|(k, _)| k.eq_ignore_ascii_case(name))).map(|(_, v)| v)
}

/// A Go `string` field: absent or `null` is `""`, other types fail like jsoniter.
fn str_field(obj: Option<&Val>, name: &str, path: &str) -> Result<String, BidderError> {
    match field(obj, name) {
        None => Ok(String::new()),
        Some(v) if v.is_null() => Ok(String::new()),
        Some(v) => match v.as_str() {
            Some(s) => Ok(s.to_string()),
            None => {
                let c = sonic_rs::to_string(v).ok().and_then(|s| s.chars().next()).unwrap_or('\0');
                Err(BidderError::FailedToUnmarshal(format!(
                    "cannot unmarshal {path}: expects \" or n, but found {c}"
                )))
            }
        },
    }
}

/// `imp.ext` -> `{"bidder": ...}` -> the bidder params object (Go `ExtImpBidder`).
/// The error is `(failed_at_bidder_step, error)`.
fn imp_bidder_params(imp: &Imp) -> Result<Option<&Val>, (bool, BidderError)> {
    let outer = obj_of(imp.ext.as_ref().map(|e| &e.0)).map_err(|e| (false, e))?;
    obj_of(field(outer, "bidder")).map_err(|e| (true, e))
}

/// Go `template.Parse` rejects an action that is not a field (`{{Malformed}}`).
fn parse_template(endpoint: &str) -> Result<EndpointTemplate, String> {
    let mut rest = endpoint;
    while let Some(open) = rest.find("{{") {
        let after = &rest[open + 2..];
        if let Some(close) = after.find("}}") {
            let action = after[..close].trim();
            if !action.starts_with('.') {
                return Err(format!(
                    "unable to parse endpoint url template: template: endpointTemplate:1: function \"{action}\" not defined"
                ));
            }
            rest = &after[close + 2..];
        } else {
            break;
        }
    }
    EndpointTemplate::parse(endpoint)
        .map_err(|e| format!("unable to parse endpoint url template: {e}"))
}
// ---------------------------------------------------------------------------------------------

/// A Go `int`/`int64` field: absent or `null` is 0. `Err(())` for a non-integer value.
fn int_field(obj: Option<&Val>, name: &str) -> Result<i64, ()> {
    match field(obj, name) {
        None => Ok(0),
        Some(v) if v.is_null() => Ok(0),
        Some(v) => v.as_i64().ok_or(()),
    }
}

/// Go `jsonutil.Unmarshal(bid.Ext, &openrtb_ext.ExtBid)` then `bidExt.Prebid.Type`:
/// `None` when the ext is absent, fails to parse or has no `prebid` object; otherwise the type
/// text (`""` when `prebid.type` is missing).
fn prebid_type(ext: Option<&Ext>) -> Option<String> {
    let ext = ext?;
    let obj = obj_of(Some(&ext.0)).ok()??;
    let prebid = field(Some(obj), "prebid")?;
    if prebid.is_null() {
        return None;
    }
    if !prebid.is_object() {
        return None;
    }
    match field(Some(prebid), "type") {
        None => Some(String::new()),
        Some(v) if v.is_null() => Some(String::new()),
        Some(v) => v.as_str().map(str::to_string),
    }
}

/// Go `openrtb_ext.ParseBidType`.
fn parse_bid_type(s: &str) -> Result<BidType, BidderError> {
    BidType::parse(s).map_err(BidderError::other)
}
