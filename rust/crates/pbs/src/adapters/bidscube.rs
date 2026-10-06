//! Go `adapters/bidscube/bidscube.go`.
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
    endpoint: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self { endpoint: endpoint.into() }
    }
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");

        let mut result = Vec::with_capacity(request.imp.len());
        let mut errs = Vec::new();
        for imp in &request.imp {
            // `map[string]json.RawMessage`: `null` is an empty map.
            let outer = match obj_of(imp.ext.as_ref().map(|e| &e.0)) {
                Ok(o) => o,
                Err(e) => {
                    errs.push(e);
                    continue;
                }
            };
            let Some(bidder_ext) = field(outer, "bidder") else {
                errs.push(BidderError::other("bidder parameters required"));
                continue;
            };

            let mut req = request.clone();
            let mut imp = imp.clone();
            imp.ext = Some(Ext(bidder_ext.clone()));
            req.imp = vec![imp];
            match crate::go_json::to_vec(&req) {
                Ok(body) => result.push(RequestData {
                    method: "POST".into(),
                    uri: self.endpoint.clone(),
                    body,
                    headers: headers.clone(),
                    imp_ids: req.imp.iter().map(|i| i.id.clone()).collect(),
                }),
                Err(e) => errs.push(BidderError::other(e.to_string())),
            }
        }
        (result, errs)
    }

    fn make_bids(
        &self,
        request: &BidRequest,
        _request_data: &RequestData,
        response_data: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        let code = response_data.status_code;
        match code {
            204 => return (None, vec![]),
            400 => return (None, vec![BidderError::bad_input(format!("unexpected status code: {code}"))]),
            200 => {}
            _ => {
                return (None, vec![BidderError::bad_server_response(format!("unexpected status code: {code}"))])
            }
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![BidderError::bad_server_response(e.to_string())]),
        };

        let mut response = BidderResponse::with_bids_capacity(request.imp.len());
        let mut errs = Vec::new();
        for seat_bid in bid_resp.seatbid {
            for bid in seat_bid.bid {
                // jsonparser.GetString(bid.Ext, "prebid", "type")
                let t = match ext_str_strict(bid.ext.as_ref()) {
                    Ok(t) => t,
                    Err(e) => {
                        errs.push(BidderError::other(format!("unable to read bid.ext.prebid.type: {e}")));
                        continue;
                    }
                };
                let bid_type = match t.as_str() {
                    "video" => BidType::Video,
                    "native" => BidType::Native,
                    _ => BidType::Banner,
                };
                response.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        (Some(response), errs)
    }
}

/// `jsonparser.GetString(ext, "prebid", "type")`.
fn ext_str_strict(ext: Option<&Ext>) -> Result<String, String> {
    let not_found = || "Key path not found".to_string();
    let ext = ext.ok_or_else(not_found)?;
    let prebid = ext.0.get("prebid").ok_or_else(not_found)?;
    let t = prebid.get("type").ok_or_else(not_found)?;
    match t.as_str() {
        Some(s) => Ok(s.to_string()),
        None => Err(format!("Value is not a string: {}", sonic_rs::to_string(t).unwrap_or_default())),
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
