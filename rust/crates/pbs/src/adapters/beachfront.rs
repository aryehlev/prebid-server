//! Go `adapters/beachfront/beachfront.go`.

#![allow(unused_imports, dead_code)]
use crate::bid_types::BidType;
use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder, BidderResponse,
    ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::macros::{EndpointTemplate, EndpointTemplateParams};
use crate::ortb::openrtb2::{Bid, BidRequest, BidResponse, Imp, MarkupType};
use crate::ortb::Ext;
use serde::{Deserialize, Serialize};

use crate::bid_types::ExtBidPrebidVideo;
use crate::ortb::adcom1::DeviceType;
use crate::ortb::openrtb2::{SupplyChain, User};
use sonic_rs::{JsonContainerTrait, JsonValueTrait};

const NURL_VIDEO_ENDPOINT_SUFFIX: &str = "&prebidserver";
const BEACHFRONT_ADAPTER_NAME: &str = "BF_PREBID_S2S";
const BEACHFRONT_ADAPTER_VERSION: &str = "1.0.0";
const MIN_BID_FLOOR: f64 = 0.01;
const DEFAULT_VIDEO_WIDTH: i64 = 300;
const DEFAULT_VIDEO_HEIGHT: i64 = 250;
const FAKE_IP: &str = "255.255.255.255";
const DEFAULT_VIDEO_ENDPOINT: &str = "https://reachms.bfmio.com/bid.json?exchange_id";

#[derive(Deserialize, Default)]
struct ExtraInfo {
    #[serde(default)]
    video_endpoint: String,
}

pub struct Adapter {
    banner_endpoint: String,
    extra_info: ExtraInfo,
}

impl Adapter {
    /// Go `Builder`; `extra_adapter_info` is `config.Adapter.ExtraAdapterInfo`.
    pub fn new(endpoint: impl Into<String>, extra_adapter_info: &str) -> Result<Self, BidderError> {
        let extra_info = get_extra_info(extra_adapter_info)?;
        Ok(Self { banner_endpoint: endpoint.into(), extra_info })
    }

    /// The video endpoint in use (Go `extraInfo.VideoEndpoint`).
    pub fn video_endpoint(&self) -> &str {
        &self.extra_info.video_endpoint
    }
}

fn get_extra_info(v: &str) -> Result<ExtraInfo, BidderError> {
    if v.is_empty() {
        return Ok(ExtraInfo { video_endpoint: DEFAULT_VIDEO_ENDPOINT.to_string() });
    }
    let mut extra: ExtraInfo =
        jsonutil::unmarshal(v.as_bytes()).map_err(|e| BidderError::other(format!("invalid extra info: {e}")))?;
    if extra.video_endpoint.is_empty() {
        extra.video_endpoint = DEFAULT_VIDEO_ENDPOINT.to_string();
    }
    Ok(extra)
}

// ---- request shapes ----

#[derive(Serialize, Default)]
struct BannerRequest {
    slots: Vec<Slot>,
    domain: String,
    page: String,
    referrer: String,
    search: String,
    secure: i8,
    #[serde(rename = "deviceOs")]
    device_os: String,
    #[serde(rename = "deviceModel")]
    device_model: String,
    #[serde(rename = "isMobile")]
    is_mobile: i8,
    ua: String,
    dnt: i8,
    user: User,
    #[serde(rename = "adapterName")]
    adapter_name: String,
    #[serde(rename = "adapterVersion")]
    adapter_version: String,
    ip: String,
    #[serde(rename = "requestId")]
    request_id: String,
    real204: bool,
    schain: BannerSchain,
}

/// Go `openrtb2.SupplyChain` written by `encoding/json`: a nil `nodes` is `null`.
#[derive(Serialize, Default)]
struct BannerSchain {
    complete: i8,
    nodes: Option<Vec<crate::ortb::openrtb2::SupplyChainNode>>,
    ver: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    ext: Option<Ext>,
}

#[derive(Serialize)]
struct Slot {
    slot: String,
    id: String,
    bidfloor: f64,
    sizes: Option<Vec<Size>>,
}

#[derive(Serialize)]
struct Size {
    w: u64,
    h: u64,
}

#[derive(Default)]
struct VideoRequest {
    app_id: String,
    video_response_type: String,
    request: BidRequest,
}

#[derive(Default)]
struct BeachfrontRequests {
    banner: BannerRequest,
    nurl_video: Vec<VideoRequest>,
    adm_video: Vec<VideoRequest>,
}

// ---- imp ext ----

#[derive(Default)]
struct ExtImpBeachfront {
    app_id: String,
    app_ids_video: String,
    app_ids_banner: String,
    bid_floor: f64,
    video_response_type: String,
}

fn found_char(v: &sonic_rs::Value) -> char {
    if v.is_object() {
        '{'
    } else if v.is_array() {
        '['
    } else if v.as_bool() == Some(true) {
        't'
    } else if v.as_bool() == Some(false) {
        'f'
    } else {
        v.to_string().chars().next().unwrap_or('0')
    }
}

fn string_field(v: &sonic_rs::Value, go_struct: &str, go_field: &str) -> Result<Option<String>, BidderError> {
    if v.is_null() {
        return Ok(None);
    }
    match v.as_str() {
        Some(s) => Ok(Some(s.to_string())),
        None => Err(BidderError::FailedToUnmarshal(format!(
            "cannot unmarshal {go_struct}.{go_field}: expects \" or n, but found {}",
            found_char(v)
        ))),
    }
}

/// Go `jsonutil.Unmarshal(bidder, &openrtb_ext.ExtImpBeachfront)`, with jsoniter's messages.
/// Fields are decoded in document order, so the first bad one is the one reported.
fn decode_ext(raw: &[u8]) -> Result<ExtImpBeachfront, BidderError> {
    if raw.is_empty() {
        return Err(BidderError::FailedToUnmarshal("expect { or n, but found \u{0}".to_string()));
    }
    let first = raw.iter().find(|b| !b" \t\r\n".contains(b)).copied().unwrap_or(0);
    if first != b'{' && first != b'n' {
        return Err(BidderError::FailedToUnmarshal(format!("expect {{ or n, but found {}", first as char)));
    }
    let mut out = ExtImpBeachfront::default();
    if first == b'n' {
        return Ok(out);
    }
    for item in sonic_rs::to_object_iter(raw) {
        let (key, lazy) = item.map_err(|e| BidderError::FailedToUnmarshal(e.to_string()))?;
        let owned: sonic_rs::Value =
            sonic_rs::from_str(lazy.as_raw_str()).map_err(|e| BidderError::FailedToUnmarshal(e.to_string()))?;
        let v = &owned;
        match key.to_ascii_lowercase().as_str() {
            "appid" => {
                if let Some(s) = string_field(v, "openrtb_ext.ExtImpBeachfront", "AppId")? {
                    out.app_id = s;
                }
            }
            "appids" => {
                if v.is_null() {
                    continue;
                }
                let Some(ids) = v.as_object() else {
                    return Err(BidderError::FailedToUnmarshal(format!(
                        "cannot unmarshal openrtb_ext.ExtImpBeachfront.AppIds: expect {{ or n, but found {}",
                        found_char(v)
                    )));
                };
                for (k, iv) in ids.iter() {
                    match k.to_ascii_lowercase().as_str() {
                        "video" => {
                            if let Some(s) = string_field(iv, "openrtb_ext.ExtImpBeachfrontAppIds", "Video")? {
                                out.app_ids_video = s;
                            }
                        }
                        "banner" => {
                            if let Some(s) = string_field(iv, "openrtb_ext.ExtImpBeachfrontAppIds", "Banner")? {
                                out.app_ids_banner = s;
                            }
                        }
                        _ => {}
                    }
                }
            }
            "bidfloor" => {
                if v.is_null() {
                    continue;
                }
                match v.as_f64() {
                    Some(f) if v.is_number() => out.bid_floor = f,
                    _ => {
                        return Err(BidderError::FailedToUnmarshal(
                            "cannot unmarshal openrtb_ext.ExtImpBeachfront.BidFloor: invalid number".to_string(),
                        ))
                    }
                }
            }
            "videoresponsetype" => {
                if let Some(s) = string_field(v, "openrtb_ext.ExtImpBeachfront", "VideoResponseType")? {
                    out.video_response_type = s;
                }
            }
            _ => {}
        }
    }
    Ok(out)
}

fn get_beachfront_extension(imp: &Imp) -> Result<ExtImpBeachfront, BidderError> {
    let bidder = imp_bidder_raw(&imp.ext).map_err(|e| {
        BidderError::bad_input(format!("ignoring imp id={}, error while decoding extImpBidder, err: {}", imp.id, e))
    })?;
    decode_ext(&bidder).map_err(|e| {
        BidderError::bad_input(format!("ignoring imp id={}, error while decoding extImpBeachfront, err: {}", imp.id, e))
    })
}

fn get_app_id(ext: &ExtImpBeachfront, media: BidType) -> Result<String, BidderError> {
    if !ext.app_id.is_empty() {
        Ok(ext.app_id.clone())
    } else if media == BidType::Video && !ext.app_ids_video.is_empty() {
        Ok(ext.app_ids_video.clone())
    } else if media == BidType::Banner && !ext.app_ids_banner.is_empty() {
        Ok(ext.app_ids_banner.clone())
    } else {
        Err(BidderError::other("unable to determine the appId(s) from the supplied extension"))
    }
}

/// Go `setBidFloor`: returns `(fatal, error)`.
fn set_bid_floor(ext: &ExtImpBeachfront, imp: &mut Imp, req_info: &ExtraRequestInfo) -> (bool, Option<BidderError>) {
    let initial = imp.bidfloor;

    if !imp.bidfloorcur.is_empty() && imp.bidfloorcur.to_uppercase() != "USD" && imp.bidfloor > 0.0 {
        let converted = req_info.convert_currency(imp.bidfloor, &imp.bidfloorcur, "USD");
        // Go assigns the (zero) result even when the conversion failed.
        imp.bidfloor = *converted.as_ref().unwrap_or(&0.0);
        let from = std::mem::replace(&mut imp.bidfloorcur, "USD".into());

        if let Err(err) = converted {
            if ext.bid_floor > MIN_BID_FLOOR {
                imp.bidfloor = ext.bid_floor;
                return (
                    false,
                    Some(BidderError::Warning(format!(
                        "The following error was recieved from the currency converter while attempting to convert the imp.bidfloor value of {:.2} from {}to USD:\n{}\nThe provided value of imp.ext.beachfront.bidfloor, {:.2} USD is being used as a fallback.",
                        initial, format_args!("{from} "), err, ext.bid_floor
                    ))),
                );
            }
            return (
                true,
                Some(BidderError::bad_input(format!(
                    "The following error was recieved from the currency converter while attempting to convert the imp.bidfloor value of {:.2} from {}to USD:\n{}\nA value of imp.ext.beachfront.bidfloor was not provided. The bid is being skipped.",
                    initial, format_args!("{from} "), err
                ))),
            );
        }
    }

    if imp.bidfloor < ext.bid_floor {
        imp.bidfloor = ext.bid_floor;
    }
    if imp.bidfloor > MIN_BID_FLOOR {
        imp.bidfloorcur = "USD".into();
    } else {
        imp.bidfloor = 0.0;
        imp.bidfloorcur = String::new();
    }
    (false, None)
}

fn get_domain(page: &str) -> String {
    let parts: Vec<&str> = page.split("//").collect();
    let domain_page = if parts.len() > 1 { parts[1] } else { parts[0] };
    domain_page.split('/').next().unwrap_or("").to_string()
}

fn is_secure(page: &str) -> i8 {
    let parts: Vec<&str> = page.split("://").collect();
    i8::from(parts.len() > 1 && parts[0] == "https")
}

fn fall_back_device_type(request: &BidRequest) -> DeviceType {
    if request.site.is_some() {
        DeviceType::PC
    } else {
        DeviceType::MOBILE
    }
}

/// Go `getBannerRequest`; `imps` is `request.Imp` (the banner imps), mutated in place as in Go.
fn get_banner_request(request: &BidRequest, imps: &mut [Imp], req_info: &ExtraRequestInfo) -> (BannerRequest, Vec<BidderError>) {
    let mut bfr = BannerRequest::default();
    let mut errs = Vec::with_capacity(imps.len());
    let mut slots: Vec<Slot> = vec![];

    for imp in imps.iter_mut() {
        let ext = match get_beachfront_extension(imp) {
            Ok(e) => e,
            Err(e) => {
                errs.push(e);
                continue;
            }
        };
        let appid = match get_app_id(&ext, BidType::Banner) {
            Ok(a) => a,
            Err(e) => {
                errs.push(e);
                continue;
            }
        };
        let (fatal, err) = set_bid_floor(&ext, imp, req_info);
        if let Some(e) = err {
            errs.push(e);
            if fatal {
                continue;
            }
        }
        let sizes: Vec<Size> = imp
            .banner
            .as_ref()
            .map(|b| b.format.iter().map(|f| Size { h: f.h as u64, w: f.w as u64 }).collect())
            .unwrap_or_default();
        slots.push(Slot {
            slot: imp.id.clone(),
            id: appid,
            bidfloor: imp.bidfloor,
            sizes: if sizes.is_empty() { None } else { Some(sizes) },
        });
    }
    if slots.is_empty() {
        return (bfr, errs);
    }
    bfr.slots = slots;

    if let Some(device) = &request.device {
        bfr.ip = device.ip.clone();
        bfr.device_model = device.model.clone();
        bfr.device_os = device.os.clone();
        if let Some(dnt) = device.dnt {
            bfr.dnt = dnt;
        }
        if !device.ua.is_empty() {
            bfr.ua = device.ua.clone();
        }
    }

    let t = fall_back_device_type(request);
    if t == DeviceType::MOBILE {
        // Go dereferences `request.App` here and panics when it is nil.
        let Some(app) = &request.app else {
            errs.push(BidderError::other("request has neither site nor app"));
            bfr.slots.clear();
            return (bfr, errs);
        };
        bfr.page = app.bundle.clone();
        bfr.domain = if app.domain.is_empty() { get_domain(&app.domain) } else { app.domain.clone() };
        bfr.is_mobile = 1;
    } else if t == DeviceType::PC {
        let site = request.site.as_ref().expect("site checked by fall_back_device_type");
        bfr.page = site.page.clone();
        bfr.domain = if site.domain.is_empty() { get_domain(&site.page) } else { site.domain.clone() };
        bfr.is_mobile = 0;
    }

    bfr.secure = is_secure(&bfr.page);

    if let Some(user) = &request.user {
        if !user.id.is_empty() && bfr.user.id.is_empty() {
            bfr.user.id = user.id.clone();
        }
        if !user.buyeruid.is_empty() && bfr.user.buyeruid.is_empty() {
            bfr.user.buyeruid = user.buyeruid.clone();
        }
    }

    bfr.request_id = request.id.clone();
    bfr.adapter_name = BEACHFRONT_ADAPTER_NAME.into();
    bfr.adapter_version = BEACHFRONT_ADAPTER_VERSION.into();

    if let Some(secure) = imps[0].secure {
        bfr.secure = secure;
    }
    bfr.real204 = true;

    if let Some(source) = &request.source {
        if let Some(src_ext) = &source.ext {
            #[derive(Deserialize, Default)]
            struct PrebidSchain {
                #[serde(default)]
                schain: SupplyChain,
            }
            if let Ok(parsed) = jsonutil::unmarshal::<PrebidSchain>(src_ext.to_json().as_bytes()) {
                let s = parsed.schain;
                bfr.schain = BannerSchain {
                    complete: s.complete,
                    nodes: if s.nodes.is_empty() { None } else { Some(s.nodes) },
                    ver: s.ver,
                    ext: s.ext,
                };
            }
        }
    }
    (bfr, errs)
}

/// Go `getVideoRequests`; `base` is the request with `Ext` cleared, `imps` the video imps.
fn get_video_requests(
    base: &BidRequest,
    imps: &[Imp],
    req_info: &ExtraRequestInfo,
) -> (Vec<VideoRequest>, Vec<BidderError>) {
    let mut bf_reqs: Vec<VideoRequest> = (0..imps.len()).map(|_| VideoRequest::default()).collect();
    let mut errs = Vec::with_capacity(imps.len());
    let mut failed: Vec<usize> = vec![];

    for i in 0..imps.len() {
        let ext = match get_beachfront_extension(&imps[i]) {
            Ok(e) => e,
            Err(e) => {
                failed.push(i);
                errs.push(e);
                continue;
            }
        };
        let appid = get_app_id(&ext, BidType::Video);
        match appid {
            Ok(a) => bf_reqs[i].app_id = a,
            Err(e) => {
                failed.push(i);
                errs.push(e);
                continue;
            }
        }

        let mut req = base.clone();
        req.imp = Vec::new();
        let mut secure: i8 = 0;

        let mut device = req.device.clone().unwrap_or_default();

        if ext.video_response_type == "nurl" {
            bf_reqs[i].video_response_type = "nurl".into();
        } else {
            bf_reqs[i].video_response_type = "adm".into();
            if device.ip.is_empty() {
                device.ip = FAKE_IP.into();
            }
        }

        if let Some(site) = req.site.as_mut() {
            if site.domain.is_empty() && !site.page.is_empty() {
                site.domain = get_domain(&site.page);
                secure = is_secure(&site.page);
            }
        }

        if let Some(app) = req.app.as_mut() {
            if app.domain.is_empty() && !app.bundle.is_empty() {
                let chunks: Vec<&str> = app.bundle.trim_matches('_').split('.').collect();
                if chunks.len() > 1 {
                    app.domain = format!("{}.{}", chunks[1], chunks[0]);
                }
            }
        }

        if device.devicetype.0 == 0 {
            device.devicetype = fall_back_device_type(base);
        }
        req.device = Some(device);

        let mut imp = imps[i].clone();
        imp.banner = None;
        imp.ext = None;
        imp.secure = Some(secure);
        let (fatal, err) = set_bid_floor(&ext, &mut imp, req_info);
        if let Some(e) = err {
            errs.push(e);
            if fatal {
                // Go leaves the whole video imp list on the request of a failed entry.
                req.imp = imps.to_vec();
                bf_reqs[i].request = req;
                failed.push(i);
                continue;
            }
        }

        if let Some(video) = imp.video.as_mut() {
            if video.w.unwrap_or_default() == 0 {
                video.w = Some(DEFAULT_VIDEO_WIDTH);
            }
            if video.h.unwrap_or_default() == 0 {
                video.h = Some(DEFAULT_VIDEO_HEIGHT);
            }
        }

        if req.cur.is_empty() {
            req.cur = vec!["USD".into()];
        }
        req.imp = vec![imp];
        bf_reqs[i].request = req;
    }

    // Go removes the failed indices one after the other, so later indices shift (kept as is).
    for idx in failed {
        if bf_reqs.len() > idx {
            bf_reqs.remove(idx);
        } else {
            bf_reqs.clear();
        }
    }
    (bf_reqs, errs)
}

/// Go `preprocess`.
fn preprocess(request: &BidRequest, req_info: &ExtraRequestInfo) -> (BeachfrontRequests, Vec<BidderError>) {
    let mut reqs = BeachfrontRequests::default();
    let mut errs = vec![];
    let mut video_imps: Vec<Imp> = vec![];
    let mut banner_imps: Vec<Imp> = vec![];

    for imp in &request.imp {
        // Go indexes `Format[0]` of a non-nil format; an empty one would panic there and is
        // treated as not valid here.
        if let Some(banner) = &imp.banner {
            if let Some(first) = banner.format.first() {
                if first.h != 0 && first.w != 0 {
                    banner_imps.push(imp.clone());
                }
            }
        }
        if imp.video.is_some() {
            video_imps.push(imp.clone());
        }
    }

    if banner_imps.len() + video_imps.len() == 0 {
        errs.push(BidderError::other("no valid impressions were found in the request"));
        return (reqs, errs);
    }

    if !banner_imps.is_empty() {
        let (banner, e) = get_banner_request(request, &mut banner_imps, req_info);
        reqs.banner = banner;
        errs = e;
    }

    if !video_imps.is_empty() {
        let mut base = request.clone();
        base.imp = Vec::new();
        base.ext = None;
        let (video_list, video_errs) = get_video_requests(&base, &video_imps, req_info);
        errs.extend(video_errs);
        for v in video_list {
            if v.video_response_type == "nurl" {
                reqs.nurl_video.push(v);
            } else if v.video_response_type == "adm" {
                reqs.adm_video.push(v);
            }
        }
    }
    (reqs, errs)
}

impl Bidder for Adapter {
    fn make_requests(&self, request: &BidRequest, req_info: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let (bf, mut errs) = preprocess(request, req_info);

        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        if let Some(device) = &request.device {
            if !device.ua.is_empty() {
                headers.add("User-Agent", device.ua.clone());
            }
            if !device.language.is_empty() {
                headers.add("Accept-Language", device.language.clone());
            }
            if let Some(dnt) = device.dnt {
                headers.add("DNT", dnt.to_string());
            }
        }

        let mut req_count = bf.adm_video.len() + bf.nurl_video.len();
        if !bf.banner.slots.is_empty() {
            req_count += 1;
        }
        // The Go header map is shared by every request, so the cookie added below reaches all.
        if let Some(user) = &request.user {
            if !user.buyeruid.is_empty() && req_count > 0 {
                headers.add("Cookie", format!("__io_cid={}", user.buyeruid));
            }
        }

        let mut reqs = Vec::with_capacity(req_count);

        if !bf.banner.slots.is_empty() {
            match crate::go_json::to_vec(&bf.banner) {
                Ok(body) => reqs.push(RequestData {
                    method: "POST".into(),
                    uri: self.banner_endpoint.clone(),
                    body,
                    headers: headers.clone(),
                    imp_ids: bf.banner.slots.iter().map(|s| s.slot.clone()).collect(),
                }),
                Err(e) => errs.push(BidderError::other(e.to_string())),
            }
        }

        for v in &bf.adm_video {
            match marshal(&v.request) {
                Ok(body) => reqs.push(RequestData {
                    method: "POST".into(),
                    uri: format!("{}={}", self.extra_info.video_endpoint, v.app_id),
                    body,
                    headers: headers.clone(),
                    imp_ids: imp_ids(&v.request.imp),
                }),
                Err(e) => errs.push(e),
            }
        }

        for v in &bf.nurl_video {
            match marshal(&v.request) {
                Ok(bytes) => {
                    let mut body = br#"{"isPrebid":true,"#.to_vec();
                    body.extend_from_slice(&bytes[1..]);
                    reqs.push(RequestData {
                        method: "POST".into(),
                        uri: format!("{}={}{}", self.extra_info.video_endpoint, v.app_id, NURL_VIDEO_ENDPOINT_SUFFIX),
                        body,
                        headers: headers.clone(),
                        imp_ids: imp_ids(&v.request.imp),
                    });
                }
                Err(e) => errs.push(e),
            }
        }

        (reqs, errs)
    }

    fn make_bids(
        &self,
        internal_request: &BidRequest,
        external_request: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if response.status_code == 204 {
            return (None, vec![]);
        }
        if response.status_code >= 500 {
            return (
                None,
                vec![BidderError::bad_server_response(format!(
                    "server error status code {} from {}. Run with request.debug = 1 for more info",
                    response.status_code, external_request.uri
                ))],
            );
        }
        if response.status_code >= 400 {
            return (
                None,
                vec![BidderError::bad_input(format!(
                    "request error status code {} from {}. Run with request.debug = 1 for more info",
                    response.status_code, external_request.uri
                ))],
            );
        }
        if response.status_code != 200 {
            return (
                None,
                vec![BidderError::other(format!(
                    "unexpected status code {} from {}. Run with request.debug = 1 for more info",
                    response.status_code, external_request.uri
                ))],
            );
        }

        let (bids, errs) = match jsonutil::unmarshal::<BidRequest>(&external_request.body) {
            Err(e) => (vec![], vec![e]),
            Ok(xtrnal) => self.postprocess(response, xtrnal, &external_request.uri, &internal_request.id),
        };
        if !errs.is_empty() {
            return (None, errs);
        }

        let mut bid_response = BidderResponse::with_bids_capacity(5);
        // Go declares `dur` once outside the loop, so a bid without `duration` keeps the last value.
        let mut dur: i64 = 0;
        for bid in bids {
            let parsed = unmarshal_raw::<BidDuration>(&ext_bytes(&bid.ext));
            let ok = parsed.is_ok();
            if let Ok(BidDuration { duration: Some(d) }) = parsed {
                dur = d;
            }
            let bid_type = self.get_bid_type(external_request);
            if ok && dur > 0 {
                let video = ExtBidPrebidVideo {
                    duration: dur as i32,
                    primary_category: bid.cat.first().cloned().unwrap_or_default(),
                };
                let mut tb = TypedBid::new(bid, bid_type);
                tb.bid_video = Some(video);
                bid_response.bids.push(tb);
            } else {
                bid_response.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        (Some(bid_response), vec![])
    }
}

#[derive(Deserialize, Default)]
struct BidDuration {
    #[serde(default, deserialize_with = "opt_i64")]
    duration: Option<i64>,
}

fn opt_i64<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<i64>, D::Error> {
    Option::<i64>::deserialize(d)
}

#[derive(Deserialize, Default)]
struct ResponseSlot {
    #[serde(default)]
    crid: String,
    #[serde(default)]
    price: f64,
    #[serde(default)]
    w: u64,
    #[serde(default)]
    h: u64,
    #[serde(default)]
    slot: String,
    #[serde(default)]
    adm: String,
}

impl Adapter {
    fn get_bid_type(&self, external_request: &RequestData) -> BidType {
        let t = external_request.uri.split('=').next().unwrap_or("");
        if t == self.extra_info.video_endpoint {
            BidType::Video
        } else {
            BidType::Banner
        }
    }

    fn postprocess(&self, response: &ResponseData, xtrnal: BidRequest, uri: &str, _id: &str) -> (Vec<Bid>, Vec<BidderError>) {
        // Go matches JSON keys case-insensitively (`seatBid` in the fixtures); the shared decoder
        // does not, so the top-level `seatbid` key is lower-cased first.
        let rtb = jsonutil::unmarshal::<BidResponse>(&lowercase_seatbid(&response.body));
        match rtb {
            Ok(r) if !r.seatbid.is_empty() => {
                let first = r.seatbid.into_iter().next().unwrap_or_default();
                postprocess_video(first.bid, &xtrnal, uri)
            }
            _ => match jsonutil::unmarshal_any::<Option<Vec<ResponseSlot>>>(&response.body) {
                Err(_) => (
                    vec![],
                    vec![BidderError::bad_server_response(
                        "server response failed to unmarshal as valid rtb. Run with request.debug = 1 for more info",
                    )],
                ),
                Ok(slots) => {
                    let bids = slots
                        .unwrap_or_default()
                        .into_iter()
                        .map(|s| Bid {
                            crid: s.crid,
                            impid: s.slot.clone(),
                            price: s.price,
                            id: format!("{}Banner", s.slot),
                            adm: s.adm,
                            h: s.h as i64,
                            w: s.w as i64,
                            ..Default::default()
                        })
                        .collect();
                    (bids, vec![])
                }
            },
        }
    }
}

fn postprocess_video(mut bids: Vec<Bid>, xtrnal: &BidRequest, uri: &str) -> (Vec<Bid>, Vec<BidderError>) {
    // Go slices `uri[len(uri)-len(suffix):]` and panics on a shorter uri; `ends_with` is false there.
    if uri.ends_with(NURL_VIDEO_ENDPOINT_SUFFIX) {
        for (i, bid) in bids.iter_mut().enumerate() {
            // Go indexes `xtrnal.Imp[i]` / `.Video` unchecked; report instead of panicking.
            let Some(imp) = xtrnal.imp.get(i) else {
                return (vec![], vec![BidderError::other("bid has no matching impression in the request")]);
            };
            let Some(video) = &imp.video else {
                return (vec![], vec![BidderError::other("matching impression has no video object")]);
            };
            match extract_nurl_video_crid(&bid.nurl) {
                Ok(crid) => bid.crid = crid,
                Err(e) => return (vec![], vec![e]),
            }
            bid.impid = imp.id.clone();
            bid.h = video.h.unwrap_or_default();
            bid.w = video.w.unwrap_or_default();
            bid.id = format!("{}NurlVideo", imp.id);
        }
    } else {
        for bid in bids.iter_mut() {
            bid.id = format!("{}AdmVideo", bid.impid);
        }
    }
    (bids, vec![])
}

fn extract_nurl_video_crid(nurl: &str) -> Result<String, BidderError> {
    // strings.SplitAfter(nurl, ":")
    let chunky: Vec<&str> = nurl.split_inclusive(':').collect();
    let chunky = if nurl.ends_with(':') || nurl.is_empty() {
        let mut c = chunky;
        c.push("");
        c
    } else {
        chunky
    };
    if chunky.len() > 1 {
        // Go indexes `chunky[2]` and panics with exactly two pieces.
        let Some(piece) = chunky.get(2) else {
            return Err(BidderError::other("unexpected nurl format"));
        };
        return Ok(piece.strip_suffix(':').unwrap_or(piece).to_string());
    }
    Ok(String::new())
}

fn lowercase_seatbid(body: &[u8]) -> Vec<u8> {
    match serde_json::from_slice::<serde_json::Value>(body) {
        Ok(serde_json::Value::Object(mut obj)) => {
            if let Some(v) = obj.remove("seatBid") {
                obj.entry("seatbid").or_insert(v);
            }
            serde_json::to_vec(&obj).unwrap_or_else(|_| body.to_vec())
        }
        _ => body.to_vec(),
    }
}

// ---- local helpers (Go `adapters.ExtImpBidder` + `jsonutil.Unmarshal` on raw ext bytes) ----

#[derive(Debug, Default, serde::Deserialize)]
struct ExtImpBidder {
    #[serde(default)]
    bidder: Option<Ext>,
}

/// Go `jsonutil.Unmarshal(raw, &v)` where `raw` may be empty (nil `json.RawMessage`): json-iterator
/// reports the NUL it reads past the end of the input. `null` leaves `v` at its zero value.
fn unmarshal_raw<T: serde::de::DeserializeOwned + Default>(raw: &[u8]) -> Result<T, BidderError> {
    if raw.is_empty() {
        return Err(BidderError::FailedToUnmarshal("expect { or n, but found \u{0}".to_string()));
    }
    if raw.iter().find(|b| !b" \t\r\n".contains(b)) == Some(&b'n') && raw.trim_ascii() == b"null" {
        return Ok(T::default());
    }
    jsonutil::unmarshal(raw)
}

fn ext_bytes(ext: &Option<Ext>) -> Vec<u8> {
    ext.as_ref().map(|e| e.to_json().into_bytes()).unwrap_or_default()
}

/// Go `jsonutil.Unmarshal(imp.Ext, &adapters.ExtImpBidder)`, returning `bidderExt.Bidder` bytes
/// (empty when absent).
fn imp_bidder_raw(ext: &Option<Ext>) -> Result<Vec<u8>, BidderError> {
    let parsed: ExtImpBidder = unmarshal_raw(&ext_bytes(ext))?;
    Ok(parsed.bidder.map(|b| b.to_json().into_bytes()).unwrap_or_default())
}

/// Go `json.Marshal(v)` into a `json.RawMessage` stand-in.
fn ext_from<T: serde::Serialize>(v: &T) -> Result<Ext, BidderError> {
    let bytes = crate::go_json::to_vec(v).map_err(|e| BidderError::other(e.to_string()))?;
    Ext::from_slice(&bytes).map_err(|e| BidderError::other(e.to_string()))
}

/// Go `openrtb_ext.GetImpIDs`.
fn imp_ids(imps: &[Imp]) -> Vec<String> {
    imps.iter().map(|i| i.id.clone()).collect()
}

/// Go `reqCopy := *request` then replacing `Imp`: a copy of the request without its imps, plus the
/// imps (cloned once).
fn split_request(request: &BidRequest) -> (BidRequest, Vec<Imp>) {
    let mut base = request.clone();
    let imps = std::mem::take(&mut base.imp);
    (base, imps)
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

fn marshal(request: &BidRequest) -> Result<Vec<u8>, BidderError> {
    crate::go_json::to_vec(request).map_err(|e| BidderError::other(e.to_string()))
}

/// jsoniter's struct-path message for a JSON string field holding a non-string value, e.g.
/// `cannot unmarshal openrtb_ext.ExtImpAJA.AdSpotID: expects " or n, but found 1`. Checked before
/// the serde decode, which words it differently. `fields` is `(json key, Go field name)`.
fn check_string_fields(raw: &[u8], go_struct: &str, fields: &[(&str, &str)]) -> Result<(), BidderError> {
    let Ok(serde_json::Value::Object(obj)) = serde_json::from_slice::<serde_json::Value>(raw) else {
        return Ok(());
    };
    for (key, go_field) in fields {
        if let Some(v) = obj.get(*key) {
            let found = match v {
                serde_json::Value::Null | serde_json::Value::String(_) => continue,
                serde_json::Value::Array(_) => '[',
                serde_json::Value::Object(_) => '{',
                serde_json::Value::Bool(true) => 't',
                serde_json::Value::Bool(false) => 'f',
                serde_json::Value::Number(n) => n.to_string().chars().next().unwrap_or('0'),
            };
            return Err(BidderError::FailedToUnmarshal(format!(
                "cannot unmarshal {go_struct}.{go_field}: expects \" or n, but found {found}"
            )));
        }
    }
    Ok(())
}
