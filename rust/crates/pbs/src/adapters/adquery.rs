//! Go `adapters/adquery/adquery.go`.
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
use serde::{Deserialize, Serialize};

const DEFAULT_CURRENCY: &str = "PLN";
const BIDDER_NAME: &str = "adquery";
const PREBID_VERSION: &str = "server";

pub struct Adapter {
    endpoint: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self { endpoint: endpoint.into() }
    }
}

#[derive(Serialize)]
struct BidderRequest<'a> {
    v: &'a str,
    #[serde(rename = "placementCode")]
    placement_code: String,
    #[serde(rename = "auctionId", skip_serializing_if = "str::is_empty")]
    auction_id: &'a str,
    #[serde(rename = "type")]
    bid_type: String,
    #[serde(rename = "adUnitCode")]
    ad_unit_code: String,
    #[serde(rename = "bidQid")]
    bid_qid: String,
    #[serde(rename = "bidId")]
    bid_id: String,
    #[serde(rename = "bidIp")]
    bid_ip: String,
    #[serde(rename = "bidIpv6")]
    bid_ipv6: String,
    #[serde(rename = "bidUa")]
    bid_ua: String,
    bidder: &'a str,
    #[serde(rename = "bidPageUrl")]
    bid_page_url: String,
    #[serde(rename = "bidderRequestId")]
    bidder_request_id: String,
    #[serde(rename = "bidRequestsCount")]
    bid_requests_count: i64,
    #[serde(rename = "bidderRequestsCount")]
    bidder_requests_count: i64,
    sizes: String,
}

#[derive(Deserialize, Default)]
struct ResponseAdQuery {
    #[serde(default)]
    data: Option<AdqResponseData>,
}

#[derive(Deserialize, Default)]
struct AdqResponseData {
    #[serde(rename = "requestId", default, deserialize_with = "crate::ortb::de::string")]
    req_id: String,
    #[serde(rename = "creationId", default)]
    cr_id: Option<serde_json::Value>,
    #[serde(default, deserialize_with = "crate::ortb::de::string")]
    currency: String,
    #[serde(default)]
    cpm: Option<String>,
    #[serde(rename = "adqLib", default)]
    adq_lib: Option<String>,
    #[serde(default)]
    tag: Option<String>,
    #[serde(rename = "adDomains", default)]
    ad_domains: Option<Vec<String>>,
    #[serde(rename = "mediaType", default)]
    media_type: Option<AdQueryMediaType>,
}

#[derive(Deserialize, Default)]
struct AdQueryMediaType {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    width: Option<String>,
    #[serde(default)]
    height: Option<String>,
}

fn build_headers(request: &BidRequest) -> Header {
    let mut headers = Header::new();
    headers.add("Content-Type", "application/json;charset=utf-8");
    headers.add("Accept", "application/json");
    headers.add("X-Openrtb-Version", "2.5");
    if let Some(d) = request.device.as_ref().filter(|d| !d.ip.is_empty()) {
        headers.add("X-Forwarded-For", d.ip.clone());
    }
    headers
}

/// Go `getImpSizes`.
fn get_imp_sizes(imp: &Imp) -> String {
    let Some(banner) = &imp.banner else { return String::new() };
    if !banner.format.is_empty() {
        return banner.format.iter().map(|f| format!("{}x{}", f.w, f.h)).collect::<Vec<_>>().join(",");
    }
    if let (Some(w), Some(h)) = (banner.w, banner.h) {
        return format!("{w}x{h}");
    }
    String::new()
}

/// Go `parseExt`.
fn parse_ext(imp: &Imp) -> Result<(String, String), BidderError> {
    let params = imp_bidder_params(imp).map_err(|(_, e)| e)?;
    let placement_id = str_field(params, "placementId", "openrtb_ext.ImpExtAdQuery.PlacementID")?;
    let ty = str_field(params, "type", "openrtb_ext.ImpExtAdQuery.Type")?;
    Ok((placement_id, ty))
}

fn go_quote(s: &str) -> String {
    format!("{s:?}")
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let headers = build_headers(request);
        let mut result = Vec::new();
        let mut errs = Vec::new();
        for imp in &request.imp {
            let (placement_id, ty) = match parse_ext(imp) {
                Ok(v) => v,
                Err(e) => {
                    errs.push(BidderError::bad_input(e.to_string()));
                    continue;
                }
            };
            let mut br = BidderRequest {
                v: PREBID_VERSION,
                placement_code: placement_id,
                auction_id: "",
                bid_type: ty,
                ad_unit_code: imp.tagid.clone(),
                bid_qid: request.user.as_ref().map(|u| u.id.clone()).unwrap_or_default(),
                bid_id: format!("{}{}", request.id, imp.id),
                bid_ip: String::new(),
                bid_ipv6: String::new(),
                bid_ua: String::new(),
                bidder: BIDDER_NAME,
                bid_page_url: String::new(),
                bidder_request_id: request.id.clone(),
                bid_requests_count: 1,
                bidder_requests_count: 1,
                sizes: get_imp_sizes(imp),
            };
            if let Some(d) = &request.device {
                br.bid_ip = d.ip.clone();
                br.bid_ipv6 = d.ipv6.clone();
                br.bid_ua = d.ua.clone();
            }
            if let Some(s) = &request.site {
                br.bid_page_url = s.page.clone();
            }
            match crate::go_json::to_vec(&br) {
                Ok(body) => result.push(RequestData {
                    method: "POST".into(),
                    uri: self.endpoint.clone(),
                    body,
                    headers: headers.clone(),
                    imp_ids: vec![imp.id.clone()],
                }),
                Err(e) => {
                    errs.push(BidderError::other(e.to_string()));
                    return (vec![], errs);
                }
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
        if is_response_status_code_no_content(response_data) {
            return (None, vec![]);
        }
        if let Some(err) = check_response_status_code_for_errors(response_data) {
            return (None, vec![err]);
        }

        // `parseResponseJson`
        if response_data.body.iter().all(|b| b" \t\r\n".contains(b)) {
            return (None, vec![unmarshal_err('\0')]);
        }
        let response: ResponseAdQuery = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let Some(data) = response.data else {
            return (Some(BidderResponse::new()), vec![]);
        };

        // `creationId` is an int64 in Go: a string fails like jsoniter's readUint64/readInt64.
        let cr_id = match &data.cr_id {
            None => 0,
            Some(v) if v.is_null() => 0,
            Some(v) => match v.as_i64() {
                Some(n) => n,
                None => {
                    let text = match v {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    let c = if v.is_string() { '"' } else { text.chars().next().unwrap_or('\0') };
                    let _ = text;
                    return (
                        None,
                        vec![BidderError::FailedToUnmarshal(format!(
                            "cannot unmarshal adquery.ResponseData.CrID: unexpected character: {c}"
                        ))],
                    );
                }
            },
        };

        let mut errs = Vec::new();
        let cpm = data.cpm.clone().unwrap_or_default();
        let price = match cpm.parse::<f64>() {
            Ok(p) => p,
            Err(_) => {
                errs.push(BidderError::other(format!("strconv.ParseFloat: parsing {}: invalid syntax", go_quote(&cpm))));
                0.0
            }
        };
        let mt = data.media_type.unwrap_or_default();
        let width_s = mt.width.clone().unwrap_or_default();
        let width = match width_s.parse::<i64>() {
            Ok(w) => w,
            Err(_) => {
                errs.push(BidderError::other(format!("strconv.ParseInt: parsing {}: invalid syntax", go_quote(&width_s))));
                0
            }
        };
        let height_s = mt.height.clone().unwrap_or_default();
        let height = match height_s.parse::<i64>() {
            Ok(h) => h,
            Err(_) => {
                errs.push(BidderError::other(format!("strconv.ParseInt: parsing {}: invalid syntax", go_quote(&height_s))));
                0
            }
        };
        let name = mt.name.clone().unwrap_or_default();
        if name != "banner" {
            return (None, vec![BidderError::other(format!("unsupported MediaType: {name}"))]);
        }
        if !errs.is_empty() {
            return (None, errs);
        }

        let mut bid_response = BidderResponse::with_bids_capacity(1);
        bid_response.currency =
            if data.currency.is_empty() { DEFAULT_CURRENCY.to_string() } else { data.currency.clone() };

        let re = match regex::Regex::new(&format!("^{}", request.id)) {
            Ok(r) => r,
            // Go `regexp.MustCompile` panics on a request id that is not a valid pattern.
            Err(e) => return (None, vec![BidderError::other(e.to_string())]),
        };
        let imp_id = re.replace_all(&data.req_id, regex::NoExpand("")).into_owned();

        let bid = Bid {
            id: data.req_id.clone(),
            impid: imp_id,
            price,
            adm: format!(
                "<script src=\"{}\"></script>{}",
                data.adq_lib.clone().unwrap_or_default(),
                data.tag.clone().unwrap_or_default()
            ),
            adomain: data.ad_domains.clone().unwrap_or_default(),
            crid: cr_id.to_string(),
            w: width,
            h: height,
            ..Default::default()
        };
        bid_response.bids.push(TypedBid::new(bid, BidType::Banner));
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
