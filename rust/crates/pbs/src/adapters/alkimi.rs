//! Go `adapters/alkimi/alkimi.go`.
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
const PRICE_MACRO: &str = "${AUCTION_PRICE}";

pub struct Adapter {
    endpoint: String,
}

/// Go `url.Parse` failure cases that matter for an endpoint: no scheme with a colon in the first
/// path segment (e.g. a leading space), or a control character.
fn url_parse_fails(s: &str) -> bool {
    if s.bytes().any(|b| b < 0x20 || b == 0x7f) {
        return true;
    }
    let has_scheme = match s.find(':') {
        Some(i) if i > 0 => {
            let scheme = &s[..i];
            scheme.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
                && scheme.chars().all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
        }
        _ => false,
    };
    if has_scheme {
        return false;
    }
    let first_segment = s.split('/').next().unwrap_or("");
    first_segment.contains(':')
}

impl Adapter {
    /// Go `Builder`: the endpoint must parse as a URL and be non-empty.
    pub fn new(endpoint: &str) -> Result<Self, String> {
        if url_parse_fails(endpoint) {
            return Err("invalid endpoint: parse error".to_string());
        }
        if endpoint.is_empty() {
            return Err("invalid endpoint: <nil>".to_string());
        }
        Ok(Self { endpoint: endpoint.to_string() })
    }
}

#[derive(serde::Deserialize, Default)]
#[serde(default)]
struct ExtImpAlkimi {
    #[serde(deserialize_with = "crate::ortb::de::string")]
    token: String,
    #[serde(rename = "bidFloor", deserialize_with = "crate::ortb::de::float")]
    bid_floor: f64,
    #[serde(deserialize_with = "crate::ortb::de::int")]
    instl: i8,
    #[serde(deserialize_with = "crate::ortb::de::int")]
    exp: i64,
}

#[derive(serde::Serialize)]
struct ExtImpAlkimiOut {
    token: String,
    #[serde(rename = "bidFloor")]
    bid_floor: f64,
    instl: i8,
    exp: i64,
    #[serde(rename = "adUnitCode")]
    ad_unit_code: String,
}

/// Go `updateImps`.
fn update_imps(request: &BidRequest) -> (Vec<Imp>, Vec<BidderError>) {
    let mut errs = Vec::new();
    let mut updated = Vec::with_capacity(request.imp.len());
    for imp in &request.imp {
        let outer = match obj_of(imp.ext.as_ref().map(|e| &e.0)) {
            Ok(o) => o,
            Err(e) => {
                errs.push(e);
                continue;
            }
        };
        let bidder_val = match obj_of(field(outer, "bidder")) {
            Ok(v) => v,
            Err(e) => {
                errs.push(e);
                continue;
            }
        };
        let params: ExtImpAlkimi = match bidder_val {
            None => ExtImpAlkimi::default(),
            Some(v) => match sonic_rs::from_value(v) {
                Ok(p) => p,
                Err(e) => {
                    errs.push(BidderError::FailedToUnmarshal(e.to_string()));
                    continue;
                }
            },
        };

        let mut imp = imp.clone();
        if !imp.bidfloorcur.is_empty() && imp.bidfloor > 0.0 {
            // keep the imp floor
        } else {
            imp.bidfloor = params.bid_floor;
        }
        imp.instl = params.instl;
        imp.exp = params.exp;

        let out = ExtImpAlkimiOut {
            token: params.token,
            bid_floor: params.bid_floor,
            instl: params.instl,
            exp: params.exp,
            ad_unit_code: imp.id.clone(),
        };
        // `map[string]json.RawMessage` is written with sorted keys.
        let mut map: std::collections::BTreeMap<String, serde_json::Value> = outer
            .and_then(|o| serde_json::from_str(&sonic_rs::to_string(o).unwrap_or_default()).ok())
            .unwrap_or_default();
        map.insert("bidder".into(), serde_json::to_value(&out).unwrap_or_default());
        let ext = match serde_json::to_vec(&map).ok().and_then(|b| Ext::from_slice(&b).ok()) {
            Some(e) => e,
            None => {
                errs.push(BidderError::other("unable to marshal imp.ext"));
                continue;
            }
        };
        imp.ext = Some(ext);
        updated.push(imp);
    }
    (updated, errs)
}

/// Go `resolveMacros`.
fn resolve_macros(bid: &mut Bid) {
    let price = format!("{}", bid.price);
    bid.nurl = bid.nurl.replace(PRICE_MACRO, &price);
    bid.adm = bid.adm.replace(PRICE_MACRO, &price);
}

fn media_type_for_imp(imp_id: &str, imps: &[Imp]) -> Result<BidType, BidderError> {
    for imp in imps {
        if imp.id == imp_id {
            if imp.banner.is_some() {
                return Ok(BidType::Banner);
            }
            if imp.video.is_some() {
                return Ok(BidType::Video);
            }
            if imp.audio.is_some() {
                return Ok(BidType::Audio);
            }
        }
    }
    Err(BidderError::bad_input(format!("Failed to find imp \"{imp_id}\"")))
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let (updated, errs) = update_imps(request);
        if !errs.is_empty() || request.imp.len() != updated.len() {
            return (vec![], errs);
        }
        let mut req = request.clone();
        req.imp = updated;
        let encoded = match crate::go_json::to_vec(&req) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::other(e.to_string())]),
        };
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        (
            vec![RequestData {
                method: "POST".into(),
                uri: self.endpoint.clone(),
                body: encoded,
                headers,
                imp_ids: req.imp.iter().map(|i| i.id.clone()).collect(),
            }],
            vec![],
        )
    }

    fn make_bids(
        &self,
        request: &BidRequest,
        _request_data: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if is_response_status_code_no_content(response) {
            return (None, vec![]);
        }
        if let Some(err) = check_response_status_code_for_errors(response) {
            return (None, vec![err]);
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        if bid_resp.seatbid.is_empty() {
            return (None, vec![BidderError::bad_server_response("Empty SeatBid array")]);
        }

        let mut bid_response = BidderResponse::with_bids_capacity(request.imp.len());
        let mut errs = Vec::new();
        for seat_bid in bid_resp.seatbid {
            for mut bid in seat_bid.bid {
                resolve_macros(&mut bid);
                match media_type_for_imp(&bid.impid, &request.imp) {
                    Ok(t) => bid_response.bids.push(TypedBid::new(bid, t)),
                    Err(e) => errs.push(e),
                }
            }
        }
        (Some(bid_response), errs)
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
