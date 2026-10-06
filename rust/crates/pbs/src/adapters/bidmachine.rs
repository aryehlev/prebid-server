//! Go `adapters/bidmachine/bidmachine.go`.
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
use crate::ortb::adcom1::CreativeAttribute;

pub struct Adapter {
    endpoint: EndpointTemplate,
}

impl Adapter {
    /// Go `Builder`: fails when the endpoint is not a valid template.
    pub fn new(endpoint: &str) -> Result<Self, String> {
        Ok(Self { endpoint: parse_template(endpoint)? })
    }

    /// Go `buildEndpointURL`.
    fn build_endpoint_url(&self, host: String, path: &str, seller_id: &str) -> Result<String, BidderError> {
        let uri_string = self
            .endpoint
            .resolve(&EndpointTemplateParams { host, ..Default::default() })
            .map_err(|_| BidderError::bad_input("Failed to resolve host macros"))?;
        let bad = || BidderError::bad_input("Failed to create final URL with provided host");
        let mut uri = url::Url::parse(&uri_string).map_err(|_| bad())?;
        if uri.host_str().is_none_or(str::is_empty) {
            return Err(bad());
        }
        // Go's `url.Parse` rejects host bytes `shouldEscape(encodeHost)` would escape (e.g. a backtick).
        if let Some(rest) = uri_string.split_once("://").map(|x| x.1) {
            let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
            let host = authority.rsplit_once('@').map_or(authority, |x| x.1);
            let ok = |c: char| !c.is_ascii() || c.is_ascii_alphanumeric() || "-_.~!$&'()*+,;=:[]<>\"%".contains(c);
            if !host.chars().all(ok) {
                return Err(bad());
            }
        }
        let joined = path_join(&[uri.path(), path]);
        let joined = path_join(&[&joined, seller_id]);
        uri.set_path(&joined);
        Ok(uri.to_string())
    }
}

/// Go `banner.Format != nil`: an explicit `[]` is present (and then "array is empty"), a missing
/// or `null` format is not.
fn format_present(imp: &Imp) -> bool {
    imp.banner.as_ref().is_some_and(|b| !b.format.is_nil())
}

/// Go `path.Join`: joins non-empty elements and cleans the result; `""` when all are empty.
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

fn has_rewarded_battr(attr: &[CreativeAttribute]) -> bool {
    attr.iter().any(|a| *a == CreativeAttribute::HAS_SKIP_BUTTON)
}

fn copy_battr_with_rewarded_inventory(src: &[CreativeAttribute]) -> Vec<CreativeAttribute> {
    let mut dst = src.to_vec();
    dst.push(CreativeAttribute::HAS_SKIP_BUTTON);
    dst
}

/// Go `GetMediaTypeForImp`: `None` is the undefined media type.
fn get_media_type_for_imp(imp_id: &str, imps: &[Imp]) -> Option<BidType> {
    for imp in imps {
        if imp.id == imp_id {
            return Some(if imp.banner.is_none() && imp.video.is_some() {
                BidType::Video
            } else {
                BidType::Banner
            });
        }
    }
    None
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json");
        headers.add("Accept", "application/json");
        headers.add("X-Openrtb-Version", "2.5");

        let mut result = Vec::with_capacity(request.imp.len());
        let mut errs = Vec::with_capacity(request.imp.len());

        for impression in &request.imp {
            if let Some(banner) = &impression.banner {
                // `banner.Format == nil` cannot be told from empty after parsing here: an absent
                // or `null` format is the "missing" case, an explicit `[]` the "empty" one. The
                // typed model keeps both as an empty Vec, so the "missing" text is used when the
                // request carries no format key at all (the only case the fixtures cover is
                // `{}` -> missing and `[]` -> empty; see `format_present`).
                if banner.w.is_none() && banner.h.is_none() && banner.format.is_empty() {
                    let msg = if format_present(impression) {
                        "banner format array is empty"
                    } else {
                        "banner format is missing"
                    };
                    errs.push(BidderError::bad_input(format!(
                        "Impression with id: {} has following error: Banner width and height is not provided and {msg}. At least one is required",
                        impression.id
                    )));
                    continue;
                }
            }

            let outer = match obj_of(impression.ext.as_ref().map(|e| &e.0)) {
                Ok(o) => o,
                Err(e) => {
                    errs.push(e);
                    continue;
                }
            };
            let params = match obj_of(field(outer, "bidder")) {
                Ok(p) => p,
                Err(e) => {
                    errs.push(e);
                    continue;
                }
            };
            let (host, path, seller_id) = match (
                str_field(params, "host", "openrtb_ext.ExtImpBidmachine.Host"),
                str_field(params, "path", "openrtb_ext.ExtImpBidmachine.Path"),
                str_field(params, "seller_id", "openrtb_ext.ExtImpBidmachine.SellerID"),
            ) {
                (Ok(h), Ok(p), Ok(s)) => (h, p, s),
                (Err(e), _, _) | (_, Err(e), _) | (_, _, Err(e)) => {
                    errs.push(e);
                    continue;
                }
            };
            let url = match self.build_endpoint_url(host, &path, &seller_id) {
                Ok(u) => u,
                Err(e) => {
                    errs.push(e);
                    continue;
                }
            };

            let mut impression = impression.clone();
            let rewarded = field(outer, "prebid")
                .and_then(|p| field(Some(p), "is_rewarded_inventory"))
                .and_then(|v| v.as_i64())
                == Some(1);
            if rewarded {
                if let Some(banner) = impression.banner.as_mut() {
                    if !has_rewarded_battr(&banner.battr) {
                        banner.battr = copy_battr_with_rewarded_inventory(&banner.battr);
                    }
                }
                if let Some(video) = impression.video.as_mut() {
                    if !has_rewarded_battr(&video.battr) {
                        video.battr = copy_battr_with_rewarded_inventory(&video.battr);
                    }
                }
            }
            let mut req = request.clone();
            req.imp = vec![impression];
            match crate::go_json::to_vec(&req) {
                Ok(body) => result.push(RequestData {
                    method: "POST".into(),
                    uri: url,
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
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        let code = response.status_code;
        let text = String::from_utf8_lossy(&response.body);
        match code {
            204 => return (None, vec![]),
            503 | 400 | 401 | 403 => {
                return (None, vec![BidderError::bad_input(format!("unexpected status code: {code} {text}"))])
            }
            200 => {}
            _ => {
                return (
                    None,
                    vec![BidderError::bad_server_response(format!("unexpected status code: {code} {text}"))],
                )
            }
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![BidderError::bad_server_response(e.to_string())]),
        };

        let mut out = BidderResponse::with_bids_capacity(request.imp.len());
        let mut errs = Vec::new();
        for seat_bid in bid_resp.seatbid {
            for bid in seat_bid.bid {
                match get_media_type_for_imp(&bid.impid, &request.imp) {
                    None => errs.push(BidderError::bad_server_response(format!(
                        "ignoring bid id={}, request doesn't contain any valid impression with id={}",
                        bid.id, bid.impid
                    ))),
                    Some(t) => out.bids.push(TypedBid::new(bid, t)),
                }
            }
        }
        (Some(out), errs)
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
