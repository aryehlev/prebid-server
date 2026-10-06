//! Go `adapters/yandex/yandex.go`.

use std::collections::HashMap;

use serde::Deserialize;
use sonic_rs::JsonValueTrait;

use crate::bid_types::BidType;
use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder,
    BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::macros::{EndpointTemplate, EndpointTemplateParams};
use crate::ortb::adcom1::MediaCreativeSubtype;
use crate::ortb::openrtb2::{Banner, BidRequest, BidResponse, Imp, Video};
use crate::ortb::Ext;

const BIDDER_VERSION: &str = "1.1";
const BIDDER_NAME: &str = "prebid.go";
const REFERER_QUERY_KEY: &str = "target-ref";
const CURRENCY_QUERY_KEY: &str = "ssp-cur";
const IMP_ID_QUERY_KEY: &str = "imp-id";
const VIDEO_MIN_DURATION: i64 = 1;
const VIDEO_MAX_DURATION: i64 = 120;

pub struct Adapter {
    endpoint: EndpointTemplate,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: &str) -> Result<Self, BidderError> {
        let template = EndpointTemplate::parse(endpoint)
            .map_err(|e| BidderError::other(format!("unable to parse endpoint url template: {e}")))?;
        // Go's `template.Parse` rejects an action that is not a field (`{{Malformed}}`).
        template
            .resolve(&EndpointTemplateParams::default())
            .map_err(|e| BidderError::other(format!("unable to parse endpoint url template: {e}")))?;
        Ok(Self { endpoint: template })
    }
}

/// Composite id of an ad placement.
struct YandexPlacementId {
    page_id: String,
    imp_id: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpBidder {
    bidder: Option<Ext>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpYandex {
    placement_id: String,
    page_id: i64,
    imp_id: i64,
}

/// Go `jsonutil.Unmarshal(raw, &v)` on a `json.RawMessage` that may be absent or not an object:
/// json-iterator reports `expect { or n, but found X` for anything but an object or `null`.
fn unmarshal_ext<T: serde::de::DeserializeOwned + Default>(ext: Option<&Ext>) -> Result<T, BidderError> {
    let Some(ext) = ext else {
        return Err(BidderError::FailedToUnmarshal("expect { or n, but found \u{0}".into()));
    };
    if ext.0.is_null() {
        return Ok(T::default());
    }
    if !ext.0.is_object() {
        let text = ext.to_json();
        let first = text.chars().next().unwrap_or('\0');
        return Err(BidderError::FailedToUnmarshal(format!("expect {{ or n, but found {first}")));
    }
    ext.decode().map_err(|e| BidderError::FailedToUnmarshal(e.to_string()))
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

fn get_yandex_placement_id(imp: &Imp) -> Result<YandexPlacementId, BidderError> {
    let ext: ExtImpBidder = unmarshal_ext(imp.ext.as_ref())
        .map_err(|_| BidderError::bad_input(format!("imp {}: unable to unmarshal ext", imp.id)))?;
    let yandex_ext: ExtImpYandex = unmarshal_ext(ext.bidder.as_ref()).map_err(|e| {
        BidderError::bad_input(format!("imp {}: unable to unmarshal ext.bidder: {e}", imp.id))
    })?;
    map_ext_to_placement_id(&yandex_ext)
}

fn map_ext_to_placement_id(yandex_ext: &ExtImpYandex) -> Result<YandexPlacementId, BidderError> {
    if yandex_ext.placement_id.is_empty() {
        return Ok(YandexPlacementId {
            imp_id: yandex_ext.imp_id.to_string(),
            page_id: yandex_ext.page_id.to_string(),
        });
    }
    // Go `strconv.Atoi`: optional sign and digits.
    let numeric: Vec<&str> = yandex_ext
        .placement_id
        .split('-')
        .filter(|part| part.parse::<i64>().is_ok())
        .collect();
    if numeric.len() < 2 {
        return Err(BidderError::bad_input(format!(
            "invalid placement id, it must contain two parts: {}",
            yandex_ext.placement_id
        )));
    }
    Ok(YandexPlacementId {
        imp_id: numeric[numeric.len() - 1].to_string(),
        page_id: numeric[numeric.len() - 2].to_string(),
    })
}

fn modify_banner(mut banner: Banner) -> Result<Banner, BidderError> {
    if banner.w.unwrap_or(0) == 0 || banner.h.unwrap_or(0) == 0 {
        let Some(first) = banner.format.first() else {
            return Err(BidderError::bad_input("Invalid size provided for Banner"));
        };
        banner.h = Some(first.h);
        banner.w = Some(first.w);
    }
    Ok(banner)
}

fn modify_video(mut video: Video) -> Result<Video, BidderError> {
    if video.w.unwrap_or_default() == 0 || video.h.unwrap_or_default() == 0 {
        return Err(BidderError::bad_input("Invalid size provided for Video"));
    }
    if video.minduration == 0 {
        video.minduration = VIDEO_MIN_DURATION;
    }
    if video.maxduration == 0 {
        video.maxduration = VIDEO_MAX_DURATION;
    }
    if video.protocols.is_empty() {
        video.protocols = vec![MediaCreativeSubtype::from(3)];
    }
    Ok(video)
}

fn modify_imp(imp: &mut Imp) -> Result<(), BidderError> {
    imp.displaymanager = BIDDER_NAME.into();
    imp.displaymanagerver = BIDDER_VERSION.into();
    let mut has_supported_type = false;
    if let Some(b) = imp.banner.take() {
        imp.banner = Some(modify_banner(b)?);
        has_supported_type = true;
    }
    if let Some(v) = imp.video.take() {
        imp.video = Some(modify_video(v)?);
        has_supported_type = true;
    }
    if imp.native.is_some() {
        has_supported_type = true;
    }
    if !has_supported_type {
        return Err(BidderError::bad_input(format!(
            "Unsupported format. Yandex only supports banner, video, and native types. Ignoring imp id #{}",
            imp.id
        )));
    }
    Ok(())
}

fn get_headers(request: &BidRequest) -> Header {
    let mut headers = Header::new();
    if let (Some(device), Some(site)) = (&request.device, &request.site) {
        let values = [
            ("Referer", site.page.as_str()),
            ("Accept-Language", device.language.as_str()),
            ("User-Agent", device.ua.as_str()),
            ("X-Forwarded-For", device.ip.as_str()),
            ("X-Real-Ip", device.ip.as_str()),
            ("Content-Type", "application/json;charset=utf-8"),
            ("Accept", "application/json"),
            ("X-OpenRTB-Version", "2.5"),
        ];
        for (k, v) in values {
            if !v.is_empty() {
                headers.add(k, v);
            }
        }
    }
    headers
}

fn get_referer(request: &BidRequest) -> String {
    match &request.site {
        None => String::new(),
        Some(site) if !site.page.is_empty() => site.page.clone(),
        Some(site) => site.domain.clone(),
    }
}

fn get_currency(request: &BidRequest) -> String {
    request.cur.first().cloned().unwrap_or_default()
}

fn get_bid_type(imp: &Imp) -> Result<BidType, BidderError> {
    if imp.video.is_some() {
        Ok(BidType::Video)
    } else if imp.native.is_some() {
        Ok(BidType::Native)
    } else if imp.banner.is_some() {
        Ok(BidType::Banner)
    } else {
        Err(BidderError::bad_input(format!(
            "Processing an invalid impression; cannot resolve impression type for imp #{}",
            imp.id
        )))
    }
}

impl Adapter {
    /// "Un-templates" the endpoint by replacing macros and adding the required query parameters.
    fn resolve_url(
        &self,
        placement_id: &YandexPlacementId,
        referer: &str,
        currency: &str,
    ) -> Result<String, BidderError> {
        let params = EndpointTemplateParams { page_id: placement_id.page_id.clone(), ..Default::default() };
        let endpoint = self.endpoint.resolve(&params).map_err(BidderError::other)?;
        // Go `url.Parse` + `Query()` + `Encode()`: existing pairs are kept, new ones added, and
        // the result is sorted by key.
        let (base, fragment) = match endpoint.split_once('#') {
            Some((b, f)) => (b.to_string(), Some(f.to_string())),
            None => (endpoint.clone(), None),
        };
        let (path, existing) = match base.split_once('?') {
            Some((p, q)) => (p.to_string(), q.to_string()),
            None => (base, String::new()),
        };
        let mut query: Vec<(String, Vec<String>)> = Vec::new();
        let mut push = |k: String, v: String| match query.iter_mut().find(|(key, _)| *key == k) {
            Some((_, vals)) => vals.push(v),
            None => query.push((k, vec![v])),
        };
        for (k, v) in url::form_urlencoded::parse(existing.as_bytes()) {
            push(k.into_owned(), v.into_owned());
        }
        for (k, v) in [
            (REFERER_QUERY_KEY, referer),
            (CURRENCY_QUERY_KEY, currency),
            (IMP_ID_QUERY_KEY, placement_id.imp_id.as_str()),
        ] {
            if !v.is_empty() {
                push(k.to_string(), v.to_string());
            }
        }
        query.sort_by(|a, b| a.0.cmp(&b.0));
        let encoded: Vec<String> = query
            .iter()
            .flat_map(|(k, vs)| vs.iter().map(move |v| format!("{}={}", query_escape(k), query_escape(v))))
            .collect();
        let mut out = path;
        if !encoded.is_empty() {
            out.push('?');
            out.push_str(&encoded.join("&"));
        }
        if let Some(f) = fragment {
            out.push('#');
            out.push_str(&f);
        }
        Ok(out)
    }
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request_data: &BidRequest,
        _request_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();
        let mut errors = Vec::new();
        let referer = get_referer(request_data);
        let currency = get_currency(request_data);
        for orig in &request_data.imp {
            let mut imp = orig.clone();
            let placement_id = match get_yandex_placement_id(&imp) {
                Ok(p) => p,
                Err(e) => {
                    errors.push(e);
                    continue;
                }
            };
            if let Err(e) = modify_imp(&mut imp) {
                errors.push(e);
                continue;
            }
            let resolved = match self.resolve_url(&placement_id, &referer, &currency) {
                Ok(u) => u,
                Err(e) => {
                    errors.push(e);
                    continue;
                }
            };
            let mut split = request_data.clone();
            split.imp = vec![imp];
            let body = match crate::go_json::to_vec(&split) {
                Ok(b) => b,
                Err(e) => {
                    errors.push(BidderError::other(e.to_string()));
                    continue;
                }
            };
            requests.push(RequestData {
                method: "POST".into(),
                uri: resolved,
                body,
                headers: get_headers(&split),
                imp_ids: split.imp.iter().map(|i| i.id.clone()).collect(),
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
        let bid_response: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(e) => {
                // Go formats the error with `%d` (a pointer to a struct): `&{%!d(string=msg)}`.
                return (
                    None,
                    vec![BidderError::bad_server_response(format!(
                        "Bad server response: &{{%!d(string={})}}",
                        e.message()
                    ))],
                );
            }
        };
        let mut out = BidderResponse::with_bids_capacity(request.imp.len());
        let mut errors = Vec::new();
        let mut imp_map: HashMap<&str, &Imp> = HashMap::new();
        for imp in &request.imp {
            imp_map.insert(&imp.id, imp);
        }
        for sb in bid_response.seatbid {
            for bid in sb.bid {
                let Some(imp) = imp_map.get(bid.impid.as_str()) else {
                    errors.push(BidderError::bad_input(format!(
                        "Invalid bid imp ID #{} does not match any imp IDs from the original bid request",
                        bid.impid
                    )));
                    continue;
                };
                match get_bid_type(imp) {
                    Ok(t) => out.bids.push(TypedBid::new(bid, t)),
                    Err(e) => errors.push(e),
                }
            }
        }
        (Some(out), errors)
    }
}
