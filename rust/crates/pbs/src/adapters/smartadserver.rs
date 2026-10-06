//! Go `adapters/smartadserver/smartadserver.go`.
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
use crate::ortb::openrtb2::{MarkupType, Publisher, Site};

pub struct Adapter {
    default_host: String,
    secondary_host: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            default_host: endpoint.into(),
            secondary_host: "https://prebid-global.smartadserver.com".to_string(),
        }
    }

    /// Go `BuildEndpointURL`.
    pub fn build_endpoint_url(&self, is_programmatic_guaranteed: bool) -> Result<String, BidderError> {
        let host = if is_programmatic_guaranteed { &self.secondary_host } else { &self.default_host };
        let malformed = || BidderError::bad_input(format!("Malformed URL: {host}."));
        let mut uri = url::Url::parse(host).map_err(|_| malformed())?;
        if uri.host_str().is_none_or(str::is_empty) {
            return Err(malformed());
        }
        if is_programmatic_guaranteed {
            let p = path_join(&[uri.path(), "ortb"]);
            uri.set_path(&p);
        } else {
            let p = path_join(&[uri.path(), "api/bid"]);
            uri.set_path(&p);
            uri.set_query(Some("callerId=5"));
        }
        Ok(uri.to_string())
    }
}

/// Go `path.Join`.
fn path_join(parts: &[&str]) -> String {
    let joined: Vec<&str> = parts.iter().copied().filter(|p| !p.is_empty()).collect();
    if joined.is_empty() {
        return String::new();
    }
    let joined = joined.join("/");
    let rooted = joined.starts_with('/');
    let mut out: Vec<&str> = Vec::new();
    for seg in joined.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                if out.last().is_some_and(|l| *l != "..") {
                    out.pop();
                } else if !rooted {
                    out.push("..");
                }
            }
            s => out.push(s),
        }
    }
    let s = out.join("/");
    if rooted {
        format!("/{s}")
    } else if s.is_empty() {
        ".".to_string()
    } else {
        s
    }
}

/// `ExtImpSmartadserverIn` / `Out`.
#[derive(Clone, Copy, serde::Serialize)]
struct SmartExt {
    #[serde(rename = "siteId")]
    site_id: i64,
    #[serde(rename = "pageId")]
    page_id: i64,
    #[serde(rename = "formatId")]
    format_id: i64,
    #[serde(rename = "networkId")]
    network_id: i64,
    #[serde(skip)]
    programmatic_guaranteed: bool,
}

fn parse_smart_ext(params: Option<&Val>) -> Result<SmartExt, ()> {
    let pg = match field(params, "programmaticGuaranteed") {
        None => false,
        Some(v) if v.is_null() => false,
        Some(v) => v.as_bool().ok_or(())?,
    };
    Ok(SmartExt {
        site_id: int_field(params, "siteId")?,
        page_id: int_field(params, "pageId")?,
        format_id: int_field(params, "formatId")?,
        network_id: int_field(params, "networkId")?,
        programmatic_guaranteed: pg,
    })
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

        let mut errs = Vec::new();
        let mut smart_request = request.clone();
        let mut site = smart_request.site.take().unwrap_or_default();
        let mut publisher = site.publisher.take().unwrap_or_default();

        let mut pending: Vec<(Imp, SmartExt)> = Vec::new();
        let mut is_programmatic_guaranteed = false;
        let mut imp_ext_key = "bidder";

        for imp in &request.imp {
            let params = match imp_bidder_params(imp) {
                Ok(p) => p,
                Err((false, _)) => {
                    errs.push(BidderError::bad_input("Error parsing bidderExt object"));
                    continue;
                }
                Err((true, _)) => {
                    errs.push(BidderError::bad_input("Error parsing smartadserverExt parameters"));
                    continue;
                }
            };
            let Ok(ext_in) = parse_smart_ext(params) else {
                errs.push(BidderError::bad_input("Error parsing smartadserverExt parameters"));
                continue;
            };
            if !is_programmatic_guaranteed && ext_in.programmatic_guaranteed {
                is_programmatic_guaranteed = true;
                imp_ext_key = "smartadserver";
            }
            publisher.id = ext_in.network_id.to_string();
            pending.push((imp.clone(), ext_in));
        }

        let mut imps = Vec::new();
        for (mut imp, ext) in pending {
            // Rewrite `imp.ext`: drop `bidder`, add `bidder` or `smartadserver` with the typed params.
            // Go marshals a `map[string]any`, so keys come out sorted.
            let mut complete: std::collections::BTreeMap<String, serde_json::Value> =
                match imp.ext.as_ref().and_then(|e| serde_json::from_str(&e.to_json()).ok()) {
                    Some(m) => m,
                    None => {
                        errs.push(BidderError::bad_input("Error parsing imp.Ext object"));
                        continue;
                    }
                };
            complete.remove("bidder");
            complete.insert(imp_ext_key.to_string(), serde_json::to_value(ext).unwrap_or_default());
            match serde_json::to_vec(&complete).ok().and_then(|b| Ext::from_slice(&b).ok()) {
                Some(e) => imp.ext = Some(e),
                None => {
                    errs.push(BidderError::bad_input("unable to marshal imp.ext"));
                    continue;
                }
            }
            imps.push(imp);
        }

        if imps.is_empty() {
            return (vec![], errs);
        }
        site.publisher = Some(publisher);
        smart_request.site = Some(site);
        smart_request.imp = imps;

        let body = match crate::go_json::to_vec(&smart_request) {
            Ok(b) => b,
            Err(_) => {
                errs.push(BidderError::bad_input("Error parsing reqJSON object"));
                return (vec![], errs);
            }
        };
        let url = match self.build_endpoint_url(is_programmatic_guaranteed) {
            Ok(u) => u,
            Err(e) => {
                errs.push(e);
                return (vec![], errs);
            }
        };
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        (
            vec![RequestData {
                method: "POST".into(),
                uri: url,
                body,
                headers,
                imp_ids: smart_request.imp.iter().map(|i| i.id.clone()).collect(),
            }],
            errs,
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
            return (
                None,
                vec![BidderError::bad_input(format!(
                    "Unexpected status code: {}. Run with request.debug = 1 for more info",
                    response.status_code
                ))],
            );
        }
        if response.status_code != 200 {
            return (
                None,
                vec![BidderError::bad_server_response(format!(
                    "Unexpected status code: {}. Run with request.debug = 1 for more info",
                    response.status_code
                ))],
            );
        }
        // jsoniter reports an empty body as `expect { or n, but found \0`; `jsonutil::unmarshal`
        // leaves empty input to serde (see /tmp/pbs-needs-b05.md).
        if response.body.is_empty() {
            return (None, vec![unmarshal_err('\0')]);
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };

        let mut bid_response = BidderResponse::with_bids_capacity(5);
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let t = match bid.mtype {
                    MarkupType::VIDEO => BidType::Video,
                    MarkupType::AUDIO => BidType::Audio,
                    MarkupType::NATIVE => BidType::Native,
                    _ => BidType::Banner,
                };
                bid_response.bids.push(TypedBid::new(bid, t));
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
