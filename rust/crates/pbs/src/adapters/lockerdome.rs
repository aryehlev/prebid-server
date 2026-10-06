//! Go `adapters/lockerdome/lockerdome.go`.
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
const UNEXPECTED_STATUS_CODE_MESSAGE: &str = "Unexpected status code: {}. Run with request.debug = 1 for more info";

fn unexpected_status(code: u16) -> String {
    UNEXPECTED_STATUS_CODE_MESSAGE.replacen("{}", &code.to_string(), 1)
}

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
        let number_of_imps = request.imp.len();
        let mut errs = Vec::new();
        if number_of_imps == 0 {
            return (vec![], vec![BidderError::bad_input("No valid impressions in the bid request.")]);
        }

        let mut valid = 0usize;
        for imp in &request.imp {
            if imp.banner.is_none() {
                errs.push(BidderError::bad_input("LockerDome does not currently support non-banner types."));
                continue;
            }
            let params = match imp_bidder_params(imp) {
                Ok(p) => p,
                Err((false, _)) => {
                    errs.push(BidderError::bad_input("ext was not provided."));
                    continue;
                }
                Err((true, _)) => {
                    errs.push(BidderError::bad_input("ext.bidder.adUnitId was not provided."));
                    continue;
                }
            };
            let Ok(ad_unit_id) = str_field(params, "adUnitId", "") else {
                errs.push(BidderError::bad_input("ext.bidder.adUnitId was not provided."));
                continue;
            };
            if ad_unit_id.is_empty() {
                errs.push(BidderError::bad_input("ext.bidder.adUnitId is empty."));
                continue;
            }
            valid += 1;
        }

        let mut req = request.clone();
        if number_of_imps > valid {
            // Go keeps the first `valid` imps (not the valid ones); same here.
            let valid_imps: Vec<Imp> = request.imp.iter().take(valid).cloned().collect();
            if valid_imps.is_empty() {
                errs.push(BidderError::bad_input("No valid or supported impressions in the bid request."));
                return (vec![], errs);
            }
            req.imp = valid_imps;
        }
        let body = match crate::go_json::to_vec(&req) {
            Ok(b) => b,
            Err(e) => {
                errs.push(BidderError::other(e.to_string()));
                return (vec![], errs);
            }
        };

        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        headers.add("x-openrtb-version", "2.5");
        // Go drops the accumulated errors when a request is built.
        (
            vec![RequestData {
                method: "POST".into(),
                uri: self.endpoint.clone(),
                body,
                headers,
                imp_ids: req.imp.iter().map(|i| i.id.clone()).collect(),
            }],
            vec![],
        )
    }

    fn make_bids(
        &self,
        _request: &BidRequest,
        _request_data: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if response.status_code == 204 {
            return (None, vec![]);
        }
        if response.status_code == 400 {
            return (None, vec![BidderError::bad_input(unexpected_status(response.status_code))]);
        }
        if response.status_code != 200 {
            return (None, vec![BidderError::bad_server_response(unexpected_status(response.status_code))]);
        }
        let resp: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(e) => {
                return (None, vec![BidderError::other(format!("Error unmarshaling LockerDome bid response - {e}"))])
            }
        };
        if resp.seatbid.is_empty() {
            return (None, vec![]);
        }
        let mut bid_response = BidderResponse::with_bids_capacity(resp.seatbid[0].bid.len());
        for seat_bid in resp.seatbid {
            for bid in seat_bid.bid {
                bid_response.bids.push(TypedBid::new(bid, BidType::Banner));
            }
        }
        (Some(bid_response), vec![])
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
