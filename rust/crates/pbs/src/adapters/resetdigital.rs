//! Go `adapters/resetdigital/resetdigital.go`.

use serde::{Deserialize, Serialize};
use sonic_rs::JsonValueTrait;

use crate::bid_types::BidType;
use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder, BidderResponse,
    ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::macros::EndpointTemplate;
use crate::ortb::openrtb2::{Bid, BidRequest, Imp};
use crate::ortb::Ext;

#[derive(Serialize, Default)]
struct ResetDigitalRequest {
    site: ResetDigitalSite,
    imps: Vec<ResetDigitalImp>,
}
#[derive(Serialize, Default)]
struct ResetDigitalSite {
    domain: String,
    referrer: String,
}
#[derive(Serialize, Default)]
struct ResetDigitalImp {
    zone_id: ResetDigitalImpZone,
    bid_id: String,
    imp_id: String,
    ext: ResetDigitalImpExt,
    media_types: ResetDigitalMediaTypes,
}
#[derive(Serialize, Default)]
struct ResetDigitalImpZone {
    #[serde(rename = "placementId")]
    placement_id: String,
}
#[derive(Serialize, Default)]
struct ResetDigitalImpExt {
    gpid: String,
}
// Go `omitempty` on a struct value never omits it.
#[derive(Serialize, Default)]
struct ResetDigitalMediaTypes {
    banner: ResetDigitalMediaType,
    video: ResetDigitalMediaType,
    audio: ResetDigitalMediaType,
}
#[derive(Serialize, Default)]
struct ResetDigitalMediaType {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    sizes: Vec<Vec<i64>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    mimes: Vec<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ResetDigitalBidResponse {
    bids: Vec<ResetDigitalBid>,
}
#[derive(Deserialize, Default)]
#[serde(default)]
struct ResetDigitalBid {
    bid_id: String,
    imp_id: String,
    cpm: f64,
    cid: String,
    crid: String,
    w: String,
    h: String,
    seat: String,
    html: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ImpExtResetDigital {
    placement_id: String,
}

pub struct Adapter {
    // Go parses the template but never sets `endpointUri`, so every request URI is empty.
    endpoint_uri: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: &str) -> Result<Self, BidderError> {
        EndpointTemplate::parse(endpoint)
            .map_err(|e| BidderError::other(format!("unable to parse endpoint url template: {e}")))?;
        Ok(Self { endpoint_uri: String::new() })
    }
}

/// Go: `Unmarshal(imp.Ext, &ExtImpBidder)` then `Unmarshal(bidderExt.Bidder, &T)`.
fn imp_bidder_params<T: serde::de::DeserializeOwned>(ext: Option<&Ext>) -> Result<T, String> {
    fn not_obj(v: &sonic_rs::Value) -> Option<String> {
        if v.is_object() || v.is_null() {
            return None;
        }
        let first = sonic_rs::to_string(v).ok()?.chars().next().unwrap_or('\u{0}');
        Some(format!("expect {{ or n, but found {first}"))
    }
    let Some(ext) = ext else {
        return Err("expect { or n, but found \u{0}".to_string());
    };
    if let Some(m) = not_obj(&ext.0) {
        return Err(m);
    }
    match ext.0.get("bidder") {
        None => Err("expect { or n, but found \u{0}".to_string()),
        Some(b) => {
            if let Some(m) = not_obj(b) {
                return Err(m);
            }
            sonic_rs::from_value::<T>(b).map_err(|e| e.to_string())
        }
    }
}

fn get_headers(request: &BidRequest) -> Header {
    let mut h = Header::new();
    let mut add = |k: &str, v: &str| {
        if !v.is_empty() {
            h.add(k, v);
        }
    };
    add("Content-Type", "application/json;charset=utf-8");
    add("Accept", "application/json");
    if let Some(d) = &request.device {
        add("Accept-Language", &d.language);
        add("User-Agent", &d.ua);
        add("X-Forwarded-For", &d.ip);
        add("X-Real-Ip", &d.ip);
    }
    if let Some(s) = &request.site {
        add("Referer", &s.page);
    }
    h
}

fn get_bid_type(imp: &Imp) -> Result<BidType, BidderError> {
    if imp.banner.is_some() {
        Ok(BidType::Banner)
    } else if imp.video.is_some() {
        Ok(BidType::Video)
    } else if imp.audio.is_some() {
        Ok(BidType::Audio)
    } else {
        Err(BidderError::other(format!("failed to find matching imp for bid {}", imp.id)))
    }
}

fn process_data_from_request(
    request: &BidRequest,
    imp: &Imp,
    bid_type: BidType,
) -> Result<ResetDigitalRequest, BidderError> {
    let mut req = ResetDigitalRequest::default();
    if let Some(site) = &request.site {
        req.site.domain = site.domain.clone();
        req.site.referrer = site.page.clone();
    }
    let mut rd = ResetDigitalImp { bid_id: request.id.clone(), imp_id: imp.id.clone(), ..Default::default() };

    if bid_type == BidType::Banner {
        if let Some(b) = &imp.banner {
            let h = b.h.unwrap_or(0);
            let w = b.w.unwrap_or(0);
            if h > 0 && w > 0 {
                rd.media_types.banner.sizes.push(vec![w, h]);
            }
        }
    }
    if bid_type == BidType::Video {
        if let Some(v) = &imp.video {
            let (h, w) = (v.h.unwrap_or_default(), v.w.unwrap_or_default());
            if h > 0 && w > 0 {
                rd.media_types.video.sizes.push(vec![w, h]);
            }
            if let Some(m) = &v.mimes {
                rd.media_types.video.mimes.extend(m.iter().cloned());
            }
        }
    }
    if bid_type == BidType::Audio {
        if let Some(a) = &imp.audio {
            if let Some(m) = &a.mimes {
                rd.media_types.audio.mimes.extend(m.iter().cloned());
            }
        }
    }

    // Go uses encoding/json here (not jsonutil), so messages are the std ones; the typed decode
    // reproduces success/failure but not the wording.
    let ext: ImpExtResetDigital = imp_bidder_params(imp.ext.as_ref()).map_err(|m| {
        // Go: `json: cannot unmarshal <kind> into Go struct field ImpExtResetDigital.placement_id of type string`.
        let kind = imp
            .ext
            .as_ref()
            .and_then(|e| e.0.get("bidder"))
            .and_then(|b| b.get("placement_id"))
            .map(|v| if v.is_number() { "number" } else if v.is_boolean() { "bool" } else if v.is_array() { "array" } else { "object" });
        match kind {
            Some(k) => BidderError::other(format!(
                "json: cannot unmarshal {k} into Go struct field ImpExtResetDigital.placement_id of type string"
            )),
            None => BidderError::other(m),
        }
    })?;
    rd.zone_id.placement_id = ext.placement_id;
    req.imps.push(rd);
    Ok(req)
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();
        let mut errors = Vec::new();
        for imp in &request.imp {
            let bid_type = match get_bid_type(imp) {
                Ok(t) => t,
                Err(e) => {
                    errors.push(e);
                    continue;
                }
            };
            let split = match process_data_from_request(request, imp, bid_type) {
                Ok(s) => s,
                Err(e) => {
                    errors.push(e);
                    continue;
                }
            };
            let body = match crate::go_json::to_vec(&split) {
                Ok(b) => b,
                Err(e) => {
                    errors.push(BidderError::other(e.to_string()));
                    continue;
                }
            };
            requests.push(RequestData {
                method: "POST".into(),
                uri: self.endpoint_uri.clone(),
                body,
                headers: get_headers(request),
                imp_ids: vec![imp.id.clone()],
            });
        }
        (requests, errors)
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
        if let Some(kind) = json_kind(&response_data.body) {
            return (
                None,
                vec![BidderError::other(format!(
                    "json: cannot unmarshal {kind} into Go value of type resetdigital.resetDigitalBidResponse"
                ))],
            );
        }
        let response: ResetDigitalBidResponse = match serde_json::from_slice(&response_data.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![BidderError::other(e.to_string())]),
        };
        if response.bids.len() != 1 {
            return (
                None,
                vec![BidderError::other(format!(
                    "expected exactly one bid in the response, but got {}",
                    response.bids.len()
                ))],
            );
        }
        let rd_bid = &response.bids[0];
        let Some(req_imp) = request.imp.iter().find(|i| i.id == rd_bid.imp_id) else {
            return (
                None,
                vec![BidderError::other(format!("no matching impression found for ImpID {}", rd_bid.imp_id))],
            );
        };

        let w = match parse_int(&rd_bid.w) {
            Ok(v) => v,
            Err(e) => return (None, vec![BidderError::other(e)]),
        };
        let h = match parse_int(&rd_bid.h) {
            Ok(v) => v,
            Err(e) => return (None, vec![BidderError::other(e)]),
        };
        let bid = Bid {
            id: rd_bid.bid_id.clone(),
            price: rd_bid.cpm,
            impid: rd_bid.imp_id.clone(),
            cid: rd_bid.cid.clone(),
            crid: rd_bid.crid.clone(),
            adm: rd_bid.html.clone(),
            w,
            h,
            ..Default::default()
        };
        let bid_type = get_media_type_for_imp(req_imp);
        let mut out = BidderResponse::with_bids_capacity(1);
        out.currency = "USD".to_string();
        let mut t = TypedBid::new(bid, bid_type);
        t.seat = rd_bid.seat.clone();
        out.bids.push(t);
        (Some(out), vec![])
    }
}

/// Go `strconv.ParseInt(s, 10, 64)` error text.
fn parse_int(s: &str) -> Result<i64, String> {
    s.parse::<i64>().map_err(|_| {
        let digits = s.strip_prefix(['+', '-']).unwrap_or(s);
        let reason =
            if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) { "value out of range" } else { "invalid syntax" };
        format!("strconv.ParseInt: parsing {s:?}: {reason}")
    })
}

/// Go `encoding/json` kind name for a top-level value that is not an object.
fn json_kind(body: &[u8]) -> Option<&'static str> {
    match body.iter().find(|b| !b" \t\r\n".contains(b))? {
        b'{' => None,
        b'"' => Some("string"),
        b'[' => Some("array"),
        b't' | b'f' => Some("bool"),
        b'n' => None,
        _ => Some("number"),
    }
}

fn get_media_type_for_imp(imp: &Imp) -> BidType {
    if imp.video.is_some() {
        return BidType::Video;
    }
    if imp.audio.is_some() {
        return BidType::Audio;
    }
    BidType::Banner
}
