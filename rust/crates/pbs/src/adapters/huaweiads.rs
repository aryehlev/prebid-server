//! Go `adapters/huaweiads/huaweiads.go` and `mcc_list.go`.
//!
//! Notes on pieces Go takes from the standard library or other packages:
//! - HMAC-SHA256 is implemented locally (`sha256` / `hmac_sha256`), since the crate has no hash
//!   dependency and shared files are not edited.
//! - Go formats `clientTime` / the nonce from the host clock in the local zone; here the zone is
//!   UTC (`+0000`), which is what Go reports on a UTC host.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use sonic_rs::JsonValueTrait;

use crate::bid_types::BidType;
use crate::bidder::{Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::native1::request::Request as NativeRequest;
use crate::ortb::native1::response::{
    Asset as NativeAsset, Data as NativeData, EventTracker as NativeEventTracker, Image as NativeImage,
    Link as NativeLink, Response as NativeResponse, Title as NativeTitle, Video as NativeVideo,
};
use crate::ortb::native1::{DataAssetType, EventTrackingMethod, EventType, ImageAssetType};
use crate::ortb::openrtb2::{Bid, BidRequest, Imp};
use crate::ortb::Ext;

const HUAWEI_ADX_API_VERSION: &str = "3.4";
const DEFAULT_COUNTRY_NAME: &str = "ZA";
const DEFAULT_UNKNOWN_NETWORK_TYPE: i32 = 0;
const DEFAULT_TIME_ZONE: &str = "+0200";
const DEFAULT_MODEL_NAME: &str = "HUAWEI";
const CHINESE_SITE_END_POINT: &str = "https://acd.op.hicloud.com/ppsadx/getResult";
const EUROPEAN_SITE_END_POINT: &str = "https://adx-dre.op.hicloud.com/ppsadx/getResult";
const ASIAN_SITE_END_POINT: &str = "https://adx-dra.op.hicloud.com/ppsadx/getResult";
const RUSSIAN_SITE_END_POINT: &str = "https://adx-drru.op.hicloud.com/ppsadx/getResult";

// creative type
const TEXT: i32 = 1;
const BIG_PICTURE: i32 = 2;
const BIG_PICTURE2: i32 = 3;
const GIF: i32 = 4;
const VIDEO_TEXT: i32 = 6;
const SMALL_PICTURE: i32 = 7;
const THREE_SMALL_PICTURES_TEXT: i32 = 8;
const VIDEO: i32 = 9;
const ICON_TEXT: i32 = 10;
const VIDEO_WITH_PICTURES_TEXT: i32 = 11;

// interaction type
const APP_PROMOTION: i32 = 3;

// ads type
const BANNER: i32 = 8;
const NATIVE: i32 = 3;
const ROLL: i32 = 60;
const INTERSTITIAL: i32 = 12;
const REWARDED: i32 = 7;
const SPLASH: i32 = 1;
const MAGAZINELOCK: i32 = 2;
const AUDIO: i32 = 17;

fn is_zero_i32(v: &i32) -> bool {
    *v == 0
}
fn is_zero_i64(v: &i64) -> bool {
    *v == 0
}
fn is_zero_f32(v: &f32) -> bool {
    *v == 0.0
}

// ---- request model (Go `huaweiAdsRequest` and friends) ----

#[derive(Serialize, Default)]
struct HuaweiAdsRequest {
    version: String,
    multislot: Vec<Adslot30>,
    app: App,
    device: Device,
    network: Network,
    regs: Regs,
    geo: Geo,
    #[serde(skip_serializing_if = "String::is_empty")]
    consent: String,
    #[serde(rename = "clientAdRequestId", skip_serializing_if = "String::is_empty")]
    client_ad_request_id: String,
}

#[derive(Serialize, Default)]
struct Adslot30 {
    slotid: String,
    adtype: i32,
    test: i32,
    #[serde(rename = "totalDuration", skip_serializing_if = "is_zero_i32")]
    total_duration: i32,
    #[serde(skip_serializing_if = "is_zero_i32")]
    orientation: i32,
    #[serde(skip_serializing_if = "is_zero_i64")]
    w: i64,
    #[serde(skip_serializing_if = "is_zero_i64")]
    h: i64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    format: Vec<Format>,
    #[serde(rename = "detailedCreativeTypeList", skip_serializing_if = "Vec::is_empty")]
    detailed_creative_type_list: Vec<String>,
}

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Hash)]
struct Format {
    #[serde(skip_serializing_if = "is_zero_i64")]
    w: i64,
    #[serde(skip_serializing_if = "is_zero_i64")]
    h: i64,
}

#[derive(Serialize, Default)]
struct App {
    #[serde(skip_serializing_if = "String::is_empty")]
    version: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    name: String,
    pkgname: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    lang: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    country: String,
}

#[derive(Serialize, Default)]
struct Device {
    #[serde(rename = "type", skip_serializing_if = "is_zero_i32")]
    r#type: i32,
    #[serde(skip_serializing_if = "String::is_empty")]
    useragent: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    os: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    version: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    maker: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    model: String,
    #[serde(skip_serializing_if = "is_zero_i32")]
    width: i32,
    #[serde(skip_serializing_if = "is_zero_i32")]
    height: i32,
    #[serde(skip_serializing_if = "String::is_empty")]
    language: String,
    #[serde(rename = "buildVersion", skip_serializing_if = "String::is_empty")]
    build_version: String,
    #[serde(skip_serializing_if = "is_zero_i32")]
    dpi: i32,
    #[serde(skip_serializing_if = "is_zero_f32")]
    pxratio: f32,
    #[serde(skip_serializing_if = "String::is_empty")]
    imei: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    oaid: String,
    #[serde(rename = "isTrackingEnabled", skip_serializing_if = "String::is_empty")]
    is_tracking_enabled: String,
    #[serde(rename = "emuiVer", skip_serializing_if = "String::is_empty")]
    emui_ver: String,
    #[serde(rename = "localeCountry")]
    locale_country: String,
    #[serde(rename = "belongCountry")]
    belong_country: String,
    #[serde(rename = "gaidTrackingEnabled", skip_serializing_if = "String::is_empty")]
    gaid_tracking_enabled: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    gaid: String,
    #[serde(rename = "clientTime")]
    client_time: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    ip: String,
}

#[derive(Serialize, Default)]
struct Network {
    r#type: i32,
    #[serde(skip_serializing_if = "is_zero_i32")]
    carrier: i32,
    #[serde(rename = "cellInfo", skip_serializing_if = "Vec::is_empty")]
    cell_info: Vec<CellInfo>,
}

#[derive(Serialize, Default)]
struct Regs {
    #[serde(skip_serializing_if = "is_zero_i32")]
    coppa: i32,
}

#[derive(Serialize, Default)]
struct Geo {
    #[serde(skip_serializing_if = "is_zero_f32")]
    lon: f32,
    #[serde(skip_serializing_if = "is_zero_f32")]
    lat: f32,
    #[serde(skip_serializing_if = "is_zero_i32")]
    accuracy: i32,
    #[serde(skip_serializing_if = "is_zero_i32")]
    lastfix: i32,
}

#[derive(Serialize, Default)]
struct CellInfo {
    #[serde(skip_serializing_if = "String::is_empty")]
    mcc: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    mnc: String,
}

// ---- response model: every field is lenient (`null` is the zero value, missing is the zero) ----

fn nd<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::de::DeserializeOwned + Default,
{
    crate::ortb::de::null_default(d)
}

macro_rules! lenient {
    ($(struct $name:ident { $( $(#[$fm:meta])* $f:ident : $t:ty ),* $(,)? })*) => {
        $(
            #[derive(Debug, Default, Deserialize)]
            struct $name {
                $( $(#[$fm])* #[serde(default, deserialize_with = "nd")] $f: $t, )*
            }
        )*
    };
}

lenient! {
    struct HuaweiAdsResponse {
        retcode: i32,
        reason: String,
        multiad: Vec<Ad30>,
    }
    struct Ad30 {
        #[serde(rename = "adtype")]
        ad_type: i32,
        slotid: String,
        retcode30: i32,
        content: Vec<Content>,
    }
    struct Content {
        contentid: String,
        interactiontype: i32,
        creativetype: i32,
        #[serde(rename = "metaData")]
        meta_data: MetaData,
        monitor: Vec<Monitor>,
        cur: String,
        price: f64,
    }
    struct MetaData {
        title: String,
        description: String,
        #[serde(rename = "imageInfo")]
        image_info: Vec<ImageInfo>,
        icon: Vec<ImageInfo>,
        #[serde(rename = "clickUrl")]
        click_url: String,
        intent: String,
        #[serde(rename = "videoInfo")]
        video_info: VideoInfo,
        duration: i64,
        #[serde(rename = "mediaFile")]
        media_file: MediaFile,
        cta: String,
    }
    struct ImageInfo {
        url: String,
        height: i64,
        width: i64,
    }
    struct VideoInfo {
        #[serde(rename = "videoDownloadUrl")]
        video_download_url: String,
        #[serde(rename = "videoDuration")]
        video_duration: i32,
        width: i32,
        height: i32,
    }
    struct MediaFile {
        mime: String,
        width: i64,
        height: i64,
        url: String,
    }
    struct Monitor {
        #[serde(rename = "eventType")]
        event_type: String,
        url: Vec<String>,
    }
}

// ---- adapter ----

#[derive(Debug, Default, Deserialize)]
struct ExtraInfo {
    #[serde(rename = "pkgNameConvert", default, deserialize_with = "nd")]
    pkg_name_convert: Vec<PkgNameConvert>,
    #[serde(rename = "closeSiteSelectionByCountry", default, deserialize_with = "nd")]
    close_site_selection_by_country: String,
}

#[derive(Debug, Default, Deserialize)]
struct PkgNameConvert {
    #[serde(rename = "convertedPkgName", default, deserialize_with = "nd")]
    converted_pkg_name: String,
    #[serde(rename = "unconvertedPkgNames", default, deserialize_with = "nd")]
    unconverted_pkg_names: Vec<String>,
    #[serde(rename = "unconvertedPkgNameKeyWords", default, deserialize_with = "nd")]
    unconverted_pkg_name_key_words: Vec<String>,
    #[serde(rename = "unconvertedPkgNamePrefixs", default, deserialize_with = "nd")]
    unconverted_pkg_name_prefixs: Vec<String>,
    #[serde(rename = "exceptionPkgNames", default, deserialize_with = "nd")]
    exception_pkg_names: Vec<String>,
}

/// Go `openrtb_ext.ExtImpHuaweiAds`.
#[derive(Debug, Default, Deserialize)]
struct ExtImpHuaweiAds {
    #[serde(default, rename = "slotid")]
    slot_id: String,
    #[serde(default)]
    adtype: String,
    #[serde(default, rename = "publisherid")]
    publisher_id: String,
    #[serde(default, rename = "signkey")]
    sign_key: String,
    #[serde(default, rename = "keyid")]
    key_id: String,
    #[serde(default, rename = "isTestAuthorization")]
    is_test_authorization: String,
}

#[derive(Debug, Default, Deserialize)]
struct ExtUserDataDeviceId {
    #[serde(default, deserialize_with = "nd")]
    imei: Vec<String>,
    #[serde(default, deserialize_with = "nd")]
    oaid: Vec<String>,
    #[serde(default, deserialize_with = "nd")]
    gaid: Vec<String>,
    #[serde(rename = "clientTime", default, deserialize_with = "nd")]
    client_time: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ExtUserDataHuaweiAds {
    #[serde(default, deserialize_with = "nd")]
    data: ExtUserDataDeviceId,
}

#[derive(Debug, Default, Deserialize)]
struct ExtUser {
    #[serde(default, deserialize_with = "nd")]
    consent: String,
}
/// Go `jsonutil.Unmarshal(ext, &target)` on a `json.RawMessage`; the failure text is the
/// json-iterator top-level one (`expect { or n, but found X`).
fn decode_ext<T: serde::de::DeserializeOwned>(ext: Option<&crate::ortb::Ext>) -> Result<T, BidderError> {
    let Some(ext) = ext else {
        // Go: Unmarshal of an empty RawMessage fails (never happens: PBS core validates imp.ext).
        return Err(BidderError::FailedToUnmarshal("unexpected end of JSON input".into()));
    };
    use sonic_rs::JsonValueTrait;
    // json-iterator picks the object decoder from the first byte: anything but `{` / `null`
    // fails with its top-level message, while serde would accept an array for a struct.
    if ext.0.is_object() || ext.0.is_null() {
        if let Ok(v) = ext.decode::<T>() {
            return Ok(v);
        }
    }
    jsonutil::unmarshal::<T>(ext.to_json().as_bytes())
}
/// Go `adapters.ExtImpBidder` (only `bidder` is used).
#[derive(Debug, Default, serde::Deserialize)]
struct ExtImpBidder {
    #[serde(default)]
    bidder: Option<crate::ortb::Ext>,
}
pub struct Adapter {
    endpoint: String,
    extra_info: ExtraInfo,
}

impl Adapter {
    /// Go `Builder`: `endpoint` is `config.Endpoint`, `extra_adapter_info` is
    /// `config.ExtraAdapterInfo` (JSON).
    pub fn new(endpoint: impl Into<String>, extra_adapter_info: &str) -> Result<Self, String> {
        let extra_info = get_extra_info(extra_adapter_info)?;
        Ok(Self { endpoint: endpoint.into(), extra_info })
    }
}

fn get_extra_info(v: &str) -> Result<ExtraInfo, String> {
    if v.is_empty() {
        return Ok(ExtraInfo::default());
    }
    let extra_info: ExtraInfo =
        jsonutil::unmarshal(v.as_bytes()).map_err(|e| format!("invalid extra info: {e} , pls check"))?;
    for convert in &extra_info.pkg_name_convert {
        if convert.converted_pkg_name.is_empty() {
            return Err("invalid extra info: ConvertedPkgName is empty, pls check".into());
        }
        for keyword in &convert.unconverted_pkg_name_key_words {
            if keyword.is_empty() {
                return Err("invalid extra info: UnconvertedPkgNameKeyWords has a empty keyword, pls check".into());
            }
        }
        for prefix in &convert.unconverted_pkg_name_prefixs {
            if prefix.is_empty() {
                return Err("invalid extra info: UnconvertedPkgNamePrefixs has a empty value, pls check".into());
            }
        }
    }
    Ok(extra_info)
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        open_rtb_request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        // the upstream code already confirms that there is a non-zero number of impressions
        let num_requests = open_rtb_request.imp.len();
        let mut request = HuaweiAdsRequest::default();
        let mut multislot = Vec::with_capacity(num_requests);

        let mut huawei_ads_imp_ext: Option<ExtImpHuaweiAds> = None;
        for imp in &open_rtb_request.imp {
            let ext = match unmarshal_ext_imp_huawei_ads(imp) {
                Ok(e) => e,
                Err(e) => return (vec![], vec![e]),
            };
            let adslot = match get_req_adslot30(&ext, imp) {
                Ok(a) => a,
                Err(e) => return (vec![], vec![e]),
            };
            huawei_ads_imp_ext = Some(ext);
            multislot.push(adslot);
        }
        request.multislot = multislot;
        request.client_ad_request_id = open_rtb_request.id.clone();

        let country_code = match get_req_json(&mut request, open_rtb_request, &self.extra_info) {
            Ok(c) => c,
            Err(e) => return (vec![], vec![e]),
        };

        let req_json = match crate::go_json::to_vec(&request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::other(e.to_string())]),
        };

        // our request header's Authorization is changing by time, cannot verify by a certain
        // string, use isTestAuthorization = true only when run testcase
        let is_test_authorization =
            huawei_ads_imp_ext.as_ref().is_some_and(|e| e.is_test_authorization == "true");
        let header = get_headers(huawei_ads_imp_ext.as_ref(), open_rtb_request, is_test_authorization);
        (
            vec![RequestData {
                method: "POST".into(),
                uri: get_final_end_point(&country_code, &self.endpoint, &self.extra_info),
                body: req_json,
                headers: header,
                imp_ids: open_rtb_request.imp.iter().map(|i| i.id.clone()).collect(),
            }],
            vec![],
        )
    }

    fn make_bids(
        &self,
        open_rtb_request: &BidRequest,
        _request_to_bidder: &RequestData,
        bidder_raw_response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if let Some(e) = check_resp_status_code(bidder_raw_response) {
            return (None, vec![e]);
        }
        // Go: a 204 passes the status check and then fails to parse the (empty) body.
        let resp: HuaweiAdsResponse = match jsonutil::unmarshal(&bidder_raw_response.body) {
            Ok(r) => r,
            Err(_) => return (None, vec![BidderError::bad_server_response("Unable to parse server response")]),
        };
        if let Some(e) = check_huawei_ads_response_retcode(&resp) {
            return (None, vec![e]);
        }
        match convert_huawei_ads_resp_to_bidder_resp(&resp, open_rtb_request) {
            Ok(r) => (Some(r), vec![]),
            Err(e) => (None, vec![e]),
        }
    }
}

/// countryCode is alpha2, choose the corresponding site end point
fn get_final_end_point(country_code: &str, default_endpoint: &str, extra_info: &ExtraInfo) -> String {
    // closeSiteSelectionByCountry == 1, close site selection, use the defaultEndpoint
    if extra_info.close_site_selection_by_country == "1" {
        return default_endpoint.to_string();
    }
    if country_code.is_empty() || country_code.len() > 2 {
        return default_endpoint.to_string();
    }
    const EUROPEAN: &[&str] = &[
        "AX", "AL", "AD", "AU", "AT", "BE", "BA", "BG", "CA", "HR", "CY", "CZ", "DK", "EE", "FO", "FI", "FR", "DE",
        "GI", "GR", "GL", "GG", "VA", "HU", "IS", "IE", "IM", "IL", "IT", "JE", "YK", "LV", "LI", "LT", "LU", "MT",
        "MD", "MC", "ME", "NL", "AN", "NZ", "NO", "PL", "PT", "RO", "MF", "VC", "SM", "RS", "SX", "SK", "SI", "ES",
        "SE", "CH", "TR", "UA", "GB", "US", "MK", "SJ", "BQ", "PM", "CW",
    ];
    if country_code == "CN" {
        CHINESE_SITE_END_POINT.to_string()
    } else if country_code == "RU" {
        RUSSIAN_SITE_END_POINT.to_string()
    } else if EUROPEAN.contains(&country_code) {
        EUROPEAN_SITE_END_POINT.to_string()
    } else {
        ASIAN_SITE_END_POINT.to_string()
    }
}

/// Go `getHeaders`: Authorization -> digest.
fn get_headers(imp_ext: Option<&ExtImpHuaweiAds>, request: &BidRequest, is_test_authorization: bool) -> Header {
    let mut headers = Header::new();
    headers.add("Content-Type", "application/json;charset=utf-8");
    headers.add("Accept", "application/json");
    let Some(imp_ext) = imp_ext else { return headers };
    headers.add("Authorization", get_digest_authorization(imp_ext, is_test_authorization));
    if let Some(device) = request.device.as_ref() {
        if !device.ua.is_empty() {
            headers.add("User-Agent", device.ua.clone());
        }
    }
    headers
}

/// Go `getReqJson`.
fn get_req_json(
    request: &mut HuaweiAdsRequest,
    open_rtb_request: &BidRequest,
    extra_info: &ExtraInfo,
) -> Result<String, BidderError> {
    request.version = HUAWEI_ADX_API_VERSION.to_string();
    let country_code = get_req_app_info(request, open_rtb_request, extra_info)?;
    get_req_device_info(request, open_rtb_request)?;
    get_req_network_info(request, open_rtb_request);
    get_req_regs_info(request, open_rtb_request);
    get_req_geo_info(request, open_rtb_request);
    get_req_consent_info(request, open_rtb_request);
    Ok(country_code)
}

fn get_req_adslot30(ext: &ExtImpHuaweiAds, imp: &Imp) -> Result<Adslot30, BidderError> {
    let adtype = convert_adtype_string_to_integer(&ext.adtype.to_lowercase());
    let test_status = if ext.is_test_authorization == "true" { 1 } else { 0 };
    let mut adslot = Adslot30 { slotid: ext.slot_id.clone(), adtype, test: test_status, ..Default::default() };
    check_and_extract_openrtb_format(&mut adslot, adtype, &ext.adtype, imp)?;
    Ok(adslot)
}

fn plain(msg: impl Into<String>) -> BidderError {
    BidderError::other(msg)
}

// opentrb :  huawei adtype
// banner <-> banner, interstitial
// native <-> native
// video  <->  banner, roll, interstitial, rewarded
fn check_and_extract_openrtb_format(
    adslot: &mut Adslot30,
    adtype: i32,
    your_adtype: &str,
    imp: &Imp,
) -> Result<(), BidderError> {
    if imp.banner.is_some() {
        if adtype != BANNER && adtype != INTERSTITIAL {
            return Err(plain(format!(
                "check openrtb format: request has banner, doesn't correspond to huawei adtype {your_adtype}"
            )));
        }
        get_banner_format(adslot, imp);
    } else if imp.native.is_some() {
        if adtype != NATIVE {
            return Err(plain(format!(
                "check openrtb format: request has native, doesn't correspond to huawei adtype {your_adtype}"
            )));
        }
        get_native_format(adslot, imp)?;
    } else if imp.video.is_some() {
        if adtype != BANNER && adtype != INTERSTITIAL && adtype != REWARDED && adtype != ROLL {
            return Err(plain(format!(
                "check openrtb format: request has video, doesn't correspond to huawei adtype {your_adtype}"
            )));
        }
        get_video_format(adslot, adtype, imp)?;
    } else if imp.audio.is_some() {
        return Err(plain("check openrtb format: request has audio, not currently supported"));
    } else {
        return Err(plain("check openrtb format: please choose one of our supported type banner, native, or video"));
    }
    Ok(())
}

fn get_banner_format(adslot: &mut Adslot30, imp: &Imp) {
    let Some(banner) = imp.banner.as_ref() else { return };
    if let (Some(w), Some(h)) = (banner.w, banner.h) {
        adslot.w = w;
        adslot.h = h;
    }
    if !banner.format.is_empty() {
        adslot.format = banner
            .format
            .iter()
            .filter(|f| f.h != 0 && f.w != 0)
            .map(|f| Format { w: f.w, h: f.h })
            .collect();
    }
}

fn get_native_format(adslot: &mut Adslot30, imp: &Imp) -> Result<(), BidderError> {
    let native = imp.native.as_ref().ok_or_else(|| plain("imp.Native is nil"))?;
    if native.request.is_empty() {
        return Err(plain("extract openrtb native failed: imp.Native.Request is empty"));
    }
    let native_payload: NativeRequest = jsonutil::unmarshal(native.request.as_bytes())?;

    // popular size for native ads
    let popular_sizes = [
        Format { w: 225, h: 150 },
        Format { w: 1080, h: 607 },
        Format { w: 300, h: 250 },
        Format { w: 1080, h: 1620 },
        Format { w: 1280, h: 720 },
        Format { w: 640, h: 360 },
        Format { w: 1080, h: 1920 },
        Format { w: 720, h: 1280 },
    ];

    // only compute the main image number, type = native1.ImageAssetTypeMain
    let mut num_main_image = 0;
    let mut num_video = 0;
    let mut formats: Vec<Format> = Vec::new();
    let mut num_format = 0;
    let mut detailed_creative_type_list: Vec<String> = Vec::with_capacity(2);

    // number of the requested image size
    for asset in &native_payload.assets {
        if num_format > 1 {
            break;
        }
        if let Some(img) = asset.img.as_ref() {
            if img.r#type == ImageAssetType::MAIN {
                num_format += 1;
            }
        }
    }

    let size_map: std::collections::HashSet<Format> = popular_sizes.iter().copied().collect();

    for asset in &native_payload.assets {
        // Only one of the {title,img,video,data} objects should be present in each object.
        if let Some(video) = asset.video.as_ref() {
            num_video += 1;
            formats = popular_sizes.to_vec();
            let (w, h) = (video.w.unwrap_or_default(), video.h.unwrap_or_default());
            if (w != 0 && h != 0) && !size_map.contains(&Format { w, h }) {
                formats.push(Format { w, h });
            }
        }
        // every image has the same W, H.
        if let Some(img) = asset.img.as_ref() {
            if img.r#type == ImageAssetType::MAIN {
                num_main_image += 1;
                if num_format > 1 && img.h != 0 && img.w != 0 && img.wmin != 0 && img.hmin != 0 {
                    formats.push(Format { w: img.w, h: img.h });
                }
                if num_format == 1 && img.h != 0 && img.w != 0 && img.wmin != 0 && img.hmin != 0 {
                    formats.extend(filter_popular_sizes(&popular_sizes, img.w, img.h, "ratio"));
                }
                if num_format == 1 && img.h == 0 && img.w == 0 && img.wmin != 0 && img.hmin != 0 {
                    formats.extend(filter_popular_sizes(&popular_sizes, img.wmin, img.hmin, "range"));
                }
            }
        }
        adslot.format = formats.clone();
    }
    if num_video >= 1 {
        detailed_creative_type_list.push("903".into());
    }
    if num_main_image >= 1 {
        detailed_creative_type_list.extend(["901".into(), "904".into(), "905".into()]);
    }
    adslot.detailed_creative_type_list = detailed_creative_type_list;
    Ok(())
}

/// filter popular size by range or ratio to append format array
fn filter_popular_sizes(sizes: &[Format], width: i64, height: i64, by_what: &str) -> Vec<Format> {
    let mut filtered = Vec::new();
    for size in sizes {
        let (w, h) = (size.w, size.h);
        if by_what == "ratio" {
            let ratio = width as f64 / height as f64;
            let diff = (w as f64 / h as f64 - ratio).abs();
            if diff <= 0.5 {
                filtered.push(*size);
            }
        }
        if by_what == "range" && w > width && h > height {
            filtered.push(*size);
        }
    }
    filtered
}

/// roll ad need TotalDuration
fn get_video_format(adslot: &mut Adslot30, adtype: i32, imp: &Imp) -> Result<(), BidderError> {
    let Some(video) = imp.video.as_ref() else { return Ok(()) };
    adslot.w = video.w.unwrap_or_default();
    adslot.h = video.h.unwrap_or_default();
    if adtype == ROLL {
        if video.maxduration == 0 {
            return Err(plain("extract openrtb video failed: MaxDuration is empty when huaweiads adtype is roll."));
        }
        adslot.total_duration = video.maxduration as i32;
    }
    Ok(())
}

fn convert_adtype_string_to_integer(adtype_lower: &str) -> i32 {
    match adtype_lower {
        "banner" => BANNER,
        "native" => NATIVE,
        "rewarded" => REWARDED,
        "interstitial" => INTERSTITIAL,
        "roll" => ROLL,
        "splash" => SPLASH,
        "magazinelock" => MAGAZINELOCK,
        "audio" => AUDIO,
        _ => BANNER,
    }
}

/// Go `getReqAppInfo`: get app information for HuaweiAds request.
fn get_req_app_info(
    request: &mut HuaweiAdsRequest,
    open_rtb_request: &BidRequest,
    extra_info: &ExtraInfo,
) -> Result<String, BidderError> {
    let mut app = App::default();
    if let Some(a) = open_rtb_request.app.as_ref() {
        if !a.ver.is_empty() {
            app.version = a.ver.clone();
        }
        if !a.name.is_empty() {
            app.name = a.name.clone();
        }
        // bundle cannot be empty, we need package name.
        if !a.bundle.is_empty() {
            app.pkgname = get_final_pkg_name(&a.bundle, extra_info);
        } else {
            return Err(plain("generate HuaweiAds AppInfo failed: openrtb BidRequest.App.Bundle is empty."));
        }
        match a.content.as_ref() {
            Some(c) if !c.language.is_empty() => app.lang = c.language.clone(),
            _ => app.lang = "en".into(),
        }
    }
    let country_code = get_country_code(open_rtb_request);
    app.country = country_code.clone();
    request.app = app;
    Ok(country_code)
}

// when it has pkgNameConvert (include different rules)
// 1. when bundleName in ExceptionPkgNames, finalPkgname = bundleName
// 2. when bundleName conform UnconvertedPkgNames, finalPkgname = ConvertedPkgName
// 3. when bundleName conform keyword, finalPkgname = ConvertedPkgName
// 4. when bundleName conform prefix, finalPkgname = ConvertedPkgName
fn get_final_pkg_name(bundle_name: &str, extra_info: &ExtraInfo) -> String {
    for convert in &extra_info.pkg_name_convert {
        if convert.converted_pkg_name.is_empty() {
            continue;
        }
        for name in &convert.exception_pkg_names {
            if name == bundle_name {
                return bundle_name.to_string();
            }
        }
        for name in &convert.unconverted_pkg_names {
            if name == bundle_name || name == "*" {
                return convert.converted_pkg_name.clone();
            }
        }
        for keyword in &convert.unconverted_pkg_name_key_words {
            if bundle_name.find(keyword.as_str()).is_some_and(|i| i > 0) {
                return convert.converted_pkg_name.clone();
            }
        }
        for prefix in &convert.unconverted_pkg_name_prefixs {
            if bundle_name.starts_with(prefix.as_str()) {
                return convert.converted_pkg_name.clone();
            }
        }
    }
    bundle_name.to_string()
}

/// Civil date from days since the Unix epoch (proleptic Gregorian).
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Go `time.Now().Format("2006-01-02 15:04:05.000")` (UTC here).
fn format_now() -> String {
    let ms = now_millis();
    let secs = ms.div_euclid(1000);
    let milli = ms.rem_euclid(1000);
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}.{milli:03}", tod / 3600, (tod % 3600) / 60, tod % 60)
}

fn client_time_regexes() -> &'static (regex::Regex, regex::Regex) {
    static RE: OnceLock<(regex::Regex, regex::Regex)> = OnceLock::new();
    RE.get_or_init(|| {
        (
            regex::Regex::new(r"^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\.\d{3}[+-]{1}\d{4}$").expect("static regex"),
            regex::Regex::new(r"^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\.\d{3}$").expect("static regex"),
        )
    })
}

/// Go `getClientTime`: field clientTime, format: 2006-01-02 15:04:05.000+0200. If this
/// parameter is not passed, the server time is used.
fn get_client_time(client_time: &str) -> String {
    // Local zone is UTC here, which Go renders as "+0000" in RFC822Z.
    let zone = "+0000";
    let _ = DEFAULT_TIME_ZONE;
    if client_time.is_empty() {
        return format!("{}{zone}", format_now());
    }
    let (with_zone, without_zone) = client_time_regexes();
    if with_zone.is_match(client_time) {
        return client_time.to_string();
    }
    if without_zone.is_match(client_time) {
        return format!("{client_time}{zone}");
    }
    format!("{}{zone}", format_now())
}

/// Go `getReqDeviceInfo`.
fn get_req_device_info(request: &mut HuaweiAdsRequest, open_rtb_request: &BidRequest) -> Result<(), BidderError> {
    let mut device = Device::default();
    if let Some(d) = open_rtb_request.device.as_ref() {
        device.r#type = d.devicetype.0 as i32;
        device.useragent = d.ua.clone();
        device.os = d.os.clone();
        device.version = d.osv.clone();
        device.maker = d.make.clone();
        device.model = d.model.clone();
        if device.model.is_empty() {
            device.model = DEFAULT_MODEL_NAME.to_string();
        }
        device.height = d.h as i32;
        device.width = d.w as i32;
        device.language = d.language.clone();
        device.pxratio = d.pxratio as f32;
        let country = get_country_code(open_rtb_request);
        device.belong_country = country.clone();
        device.locale_country = country;
        device.ip = d.ip.clone();
        device.gaid = d.ifa.clone();
        device.client_time = get_client_time("");
    }

    // get oaid gaid imei in openRTBRequest.User.Ext.Data
    get_device_id_from_user_ext(&mut device, open_rtb_request)?;

    // IsTrackingEnabled = 1 - DNT
    if let Some(dnt) = open_rtb_request.device.as_ref().and_then(|d| d.dnt) {
        if !device.oaid.is_empty() {
            device.is_tracking_enabled = (1 - i64::from(dnt)).to_string();
        }
        if !device.gaid.is_empty() {
            device.gaid_tracking_enabled = (1 - i64::from(dnt)).to_string();
        }
    }

    request.device = device;
    Ok(())
}

fn get_country_code(request: &BidRequest) -> String {
    if let Some(c) = request.device.as_ref().and_then(|d| d.geo.as_ref()).map(|g| &g.country).filter(|c| !c.is_empty())
    {
        convert_country_code(c)
    } else if let Some(c) =
        request.user.as_ref().and_then(|u| u.geo.as_ref()).map(|g| &g.country).filter(|c| !c.is_empty())
    {
        convert_country_code(c)
    } else if let Some(m) = request.device.as_ref().map(|d| &d.mccmnc).filter(|m| !m.is_empty()) {
        get_country_code_from_mcc(m)
    } else {
        DEFAULT_COUNTRY_NAME.to_string()
    }
}

/// ISO 3166-1 Alpha3 -> Alpha2, Some countries may use
fn convert_country_code(country: &str) -> String {
    if country.is_empty() {
        return DEFAULT_COUNTRY_NAME.to_string();
    }
    const MAP: &[(&str, &str)] = &[
        ("AND", "AD"), ("AGO", "AO"), ("AUT", "AT"), ("BGD", "BD"), ("BLR", "BY"), ("CAF", "CF"), ("TCD", "TD"),
        ("CHL", "CL"), ("CHN", "CN"), ("COG", "CG"), ("COD", "CD"), ("DNK", "DK"), ("GNQ", "GQ"), ("EST", "EE"),
        ("GIN", "GN"), ("GNB", "GW"), ("GUY", "GY"), ("IRQ", "IQ"), ("IRL", "IE"), ("ISR", "IL"), ("KAZ", "KZ"),
        ("LBY", "LY"), ("MDG", "MG"), ("MDV", "MV"), ("MEX", "MX"), ("MNE", "ME"), ("MOZ", "MZ"), ("PAK", "PK"),
        ("PNG", "PG"), ("PRY", "PY"), ("POL", "PL"), ("PRT", "PT"), ("SRB", "RS"), ("SVK", "SK"), ("SVN", "SI"),
        ("SWE", "SE"), ("TUN", "TN"), ("TUR", "TR"), ("TKM", "TM"), ("UKR", "UA"), ("ARE", "AE"), ("URY", "UY"),
    ];
    if let Some((_, v)) = MAP.iter().find(|(k, _)| *k == country) {
        return (*v).to_string();
    }
    if country.len() >= 2 {
        // Go slices bytes: country[0:2]
        return String::from_utf8_lossy(&country.as_bytes()[..2]).into_owned();
    }
    DEFAULT_COUNTRY_NAME.to_string()
}

fn get_country_code_from_mcc(mcc: &str) -> String {
    let country_mcc = mcc.split('-').next().unwrap_or("");
    match country_mcc.parse::<i64>() {
        Err(_) => DEFAULT_COUNTRY_NAME.to_string(),
        Ok(v) => match MCC_LIST.iter().find(|(k, _)| *k == v) {
            Some((_, c)) => c.to_uppercase(),
            None => DEFAULT_COUNTRY_NAME.to_string(),
        },
    }
}

/// json-iterator's message for a slice field that is not an array. json-iterator decodes keys in
/// document order, so the first offending key in the object is the one reported.
fn check_slice_fields(obj: &sonic_rs::Value) -> Result<(), BidderError> {
    use sonic_rs::JsonContainerTrait;
    let Some(map) = obj.as_object() else { return Ok(()) };
    for (key, v) in map.iter() {
        let name = match key {
            "imei" => "Imei",
            "oaid" => "Oaid",
            "gaid" => "Gaid",
            "clientTime" => "ClientTime",
            _ => continue,
        };
        if !v.is_array() && !v.is_null() {
            let found = v.to_string().chars().next().unwrap_or(' ');
            return Err(BidderError::FailedToUnmarshal(format!(
                "cannot unmarshal openrtb_ext.ExtUserDataDeviceIdHuaweiAds.{name}: decode slice: expect [ or n, but found {found}"
            )));
        }
    }
    Ok(())
}

fn unmarshal_user_data(ext: &Ext) -> Result<ExtUserDataHuaweiAds, BidderError> {
    if !(ext.0.is_object() || ext.0.is_null()) {
        return jsonutil::unmarshal(ext.to_json().as_bytes());
    }
    if let Some(data) = ext.0.get("data") {
        if data.is_object() {
            check_slice_fields(data)?;
        } else if !data.is_null() {
            let found = data.to_string().chars().next().unwrap_or(' ');
            return Err(BidderError::FailedToUnmarshal(format!(
                "cannot unmarshal openrtb_ext.ExtUserDataHuaweiAds.Data: expect {{ or n, but found {found}"
            )));
        }
    }
    jsonutil::unmarshal(ext.to_json().as_bytes())
}

/// getDeviceID include oaid gaid imei. In prebid mobile, use TargetingParams.addUserData("imei", "imei-test");
/// When ifa: gaid exists, other device id can be passed by TargetingParams.addUserData("oaid", "oaid-test");
fn get_device_id_from_user_ext(device: &mut Device, request: &BidRequest) -> Result<(), BidderError> {
    let user_ext = request.user.as_ref().and_then(|u| u.ext.as_ref());
    if let Some(ext) = user_ext {
        let ext_user_data = unmarshal_user_data(ext).map_err(|e| {
            plain(format!(
                "get gaid from openrtb Device.IFA failed, and get device id failed: Unmarshal openRTBRequest.User.Ext -> extUserDataHuaweiAds. Error: {e}"
            ))
        })?;
        let device_id = ext_user_data.data;
        let mut is_valid_device_id = false;

        if let Some(v) = device_id.oaid.first() {
            device.oaid = v.clone();
            is_valid_device_id = true;
        }
        if let Some(v) = device_id.gaid.first() {
            device.gaid = v.clone();
            is_valid_device_id = true;
        }
        if !device.gaid.is_empty() {
            is_valid_device_id = true;
        }
        if let Some(v) = device_id.imei.first() {
            device.imei = v.clone();
            is_valid_device_id = true;
        }
        if !is_valid_device_id {
            return Err(plain("getDeviceID: Imei ,Oaid, Gaid are all empty."));
        }
        if let Some(v) = device_id.client_time.first() {
            device.client_time = get_client_time(v);
        }
    } else if device.gaid.is_empty() {
        return Err(plain("getDeviceID: openRTBRequest.User.Ext is nil and device.Gaid is not specified."));
    }
    Ok(())
}

/// Go `getReqNetWorkInfo`: for HuaweiAds request, include Carrier, Mcc, Mnc.
fn get_req_network_info(request: &mut HuaweiAdsRequest, open_rtb_request: &BidRequest) {
    let Some(d) = open_rtb_request.device.as_ref() else { return };
    let mut network = Network {
        r#type: d.connectiontype.map_or(DEFAULT_UNKNOWN_NETWORK_TYPE, |c| i32::from(c.0)),
        ..Default::default()
    };
    let mut cell_infos = Vec::new();
    if !d.mccmnc.is_empty() {
        let arr: Vec<&str> = d.mccmnc.split('-').collect();
        network.carrier = 0;
        if arr.len() >= 2 {
            cell_infos.push(CellInfo { mcc: arr[0].to_string(), mnc: arr[1].to_string() });
            let s = format!("{}{}", arr[0], arr[1]);
            network.carrier = match s.as_str() {
                "46000" | "46002" | "46007" => 2,
                "46001" | "46006" => 1,
                "46003" | "46005" | "46011" => 3,
                _ => 99,
            };
        }
    }
    network.cell_info = cell_infos;
    request.network = network;
}

/// Go `getReqRegsInfo`: coppa.
fn get_req_regs_info(request: &mut HuaweiAdsRequest, open_rtb_request: &BidRequest) {
    if let Some(regs) = open_rtb_request.regs.as_ref() {
        let coppa = regs.coppa;
        if coppa >= 0 {
            request.regs = Regs { coppa: i32::from(coppa) };
        }
    }
}

/// Go `getReqGeoInfo`: Lon, Lat, Accuracy, Lastfix.
fn get_req_geo_info(request: &mut HuaweiAdsRequest, open_rtb_request: &BidRequest) {
    if let Some(geo) = open_rtb_request.device.as_ref().and_then(|d| d.geo.as_ref()) {
        request.geo = Geo {
            lon: geo.lon.unwrap_or_default() as f32,
            lat: geo.lat.unwrap_or_default() as f32,
            accuracy: geo.accuracy as i32,
            lastfix: geo.lastfix as i32,
        };
    }
}

/// Go `getReqConsentInfo`: GDPR consent.
fn get_req_consent_info(request: &mut HuaweiAdsRequest, open_rtb_request: &BidRequest) {
    if let Some(ext) = open_rtb_request.user.as_ref().and_then(|u| u.ext.as_ref()) {
        if let Ok(ext_user) = decode_ext::<ExtUser>(Some(ext)) {
            request.consent = ext_user.consent;
        }
    }
}

fn unmarshal_ext_imp_huawei_ads(imp: &Imp) -> Result<ExtImpHuaweiAds, BidderError> {
    let bidder_ext: ExtImpBidder = decode_ext(imp.ext.as_ref())
        .map_err(|_| plain("Unmarshal: openRTBImp.Ext -> bidderExt failed"))?;
    let ext: ExtImpHuaweiAds = decode_ext(bidder_ext.bidder.as_ref())
        .map_err(|_| plain("Unmarshal: bidderExt.Bidder -> huaweiAdsImpExt failed"))?;
    if ext.slot_id.is_empty() {
        return Err(plain("ExtImpHuaweiAds: slotid is empty."));
    }
    if ext.adtype.is_empty() {
        return Err(plain("ExtImpHuaweiAds: adtype is empty."));
    }
    if ext.publisher_id.is_empty() {
        return Err(plain("ExtHuaweiAds: publisherid is empty."));
    }
    if ext.sign_key.is_empty() {
        return Err(plain("ExtHuaweiAds: signkey is empty."));
    }
    if ext.key_id.is_empty() {
        return Err(plain("ExtImpHuaweiAds: keyid is empty."));
    }
    Ok(ext)
}

fn check_resp_status_code(response: &ResponseData) -> Option<BidderError> {
    if response.status_code == 204 {
        return None;
    }
    if response.status_code == 503 {
        return Some(BidderError::bad_input(format!(
            "Something went wrong, please contact your Account Manager. Status Code: [ {} ] ",
            response.status_code
        )));
    }
    if response.status_code != 200 {
        return Some(BidderError::bad_input(format!(
            "Unexpected status code: [ {} ]. Run with request.debug = 1 for more info",
            response.status_code
        )));
    }
    // Go also fails on a nil body; an empty body is not distinguishable from nil here and fails
    // later at parsing with the same outcome for the caller (an error).
    None
}

fn check_huawei_ads_response_retcode(response: &HuaweiAdsResponse) -> Option<BidderError> {
    let rc = response.retcode;
    if rc == 200 || rc == 206 {
        return None;
    }
    if rc == 204 {
        return Some(BidderError::bad_input(format!(
            "HuaweiAdsResponse retcode: {rc} , reason: The request packet is correct, but no advertisement was found for this request."
        )));
    }
    if (400..600).contains(&rc) || (rc < 300 && rc > 200) {
        return Some(BidderError::bad_input(format!("HuaweiAdsResponse retcode: {rc} , reason: {}", response.reason)));
    }
    None
}

/// Go `convertHuaweiAdsRespToBidderResp`.
fn convert_huawei_ads_resp_to_bidder_resp(
    resp: &HuaweiAdsResponse,
    request: &BidRequest,
) -> Result<BidderResponse, BidderError> {
    if resp.multiad.is_empty() {
        return Err(plain(
            "convert huaweiads response to bidder response failed: multiad length is 0, get no ads from huawei side.",
        ));
    }
    let mut bidder_response = BidderResponse::with_bids_capacity(resp.multiad.len());
    // Default Currency: CNY
    bidder_response.currency = "CNY".into();

    // record request Imp (slotid->imp, slotid->openrtb_ext.bidtype)
    let mut slot2imp: HashMap<String, &Imp> = HashMap::with_capacity(request.imp.len());
    let mut slot2media: HashMap<String, BidType> = HashMap::with_capacity(request.imp.len());
    for imp in &request.imp {
        let Ok(ext) = unmarshal_ext_imp_huawei_ads(imp) else { continue };
        slot2imp.insert(ext.slot_id.clone(), imp);
        let mut media_type = BidType::Banner;
        if imp.video.is_some() {
            media_type = BidType::Video;
        } else if imp.native.is_some() {
            media_type = BidType::Native;
        } else if imp.audio.is_some() {
            media_type = BidType::Audio;
        }
        slot2media.insert(ext.slot_id, media_type);
    }
    if slot2media.is_empty() || slot2imp.is_empty() {
        return Err(plain("convert huaweiads response to bidder response failed: openRTBRequest.imp is nil"));
    }

    for ad30 in &resp.multiad {
        let imp_id = slot2imp.get(&ad30.slotid).map(|i| i.id.as_str()).unwrap_or("");
        if imp_id.is_empty() {
            continue;
        }
        if ad30.retcode30 != 200 {
            continue;
        }
        let imp = slot2imp[&ad30.slotid];
        let media_type = slot2media.get(&ad30.slotid).copied().unwrap_or(BidType::Banner);
        for content in &ad30.content {
            let mut bid = Bid { id: imp_id.to_string(), impid: imp_id.to_string(), ..Default::default() };
            // The bidder has already helped us automatically convert the currency price, here only
            // the CNY price is filled in
            bid.price = content.price;
            bid.crid = content.contentid.clone();
            // All currencies should be the same
            if !content.cur.is_empty() {
                bidder_response.currency = content.cur.clone();
            }
            let (adm, w, h) = handle_huawei_ads_content(ad30.ad_type, content, media_type, imp)?;
            bid.adm = adm;
            bid.w = w;
            bid.h = h;
            bid.adomain.push("huaweiads".into());
            bid.nurl = get_nurl(content);
            bidder_response.bids.push(TypedBid::new(bid, media_type));
        }
    }
    Ok(bidder_response)
}

fn get_nurl(content: &Content) -> String {
    for monitor in &content.monitor {
        if monitor.event_type == "win" && !monitor.url.is_empty() {
            return monitor.url[0].clone();
        }
    }
    String::new()
}

type AdmResult = Result<(String, i64, i64), BidderError>;

/// handleHuaweiAdsContent: get field Adm, Width, Height
fn handle_huawei_ads_content(ad_type: i32, content: &Content, bid_type: BidType, imp: &Imp) -> AdmResult {
    let res = match bid_type {
        BidType::Banner => extract_adm_banner(ad_type, content, bid_type, imp),
        BidType::Native => extract_adm_native(ad_type, content, bid_type, imp),
        BidType::Video => extract_adm_video(ad_type, content, bid_type, imp),
        _ => return Err(plain("no support bidtype: audio")),
    };
    res.map_err(|e| plain(format!("generate Adm field from HuaweiAds response failed: {e}")))
}

fn extract_adm_banner(ad_type: i32, content: &Content, bid_type: BidType, imp: &Imp) -> AdmResult {
    // support openrtb: banner  <=> huawei adtype: banner, interstitial
    if ad_type != BANNER && ad_type != INTERSTITIAL {
        return Err(plain("openrtb banner should correspond to huaweiads adtype: banner or interstitial"));
    }
    let mut creative_type = content.creativetype;
    if content.creativetype > 100 {
        creative_type -= 100;
    }
    if [TEXT, BIG_PICTURE, BIG_PICTURE2, SMALL_PICTURE, THREE_SMALL_PICTURES_TEXT, ICON_TEXT, GIF]
        .contains(&creative_type)
    {
        extract_adm_picture(content)
    } else if creative_type == VIDEO_TEXT || creative_type == VIDEO || creative_type == VIDEO_WITH_PICTURES_TEXT {
        extract_adm_video(ad_type, content, bid_type, imp)
    } else {
        Err(plain("no banner support creativetype"))
    }
}

fn get_decode_value(s: &str) -> String {
    if s.is_empty() {
        return String::new();
    }
    // Go `url.QueryUnescape`: '+' is a space, a bad %-escape is an error (-> "").
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' => {
                let hex = |c: Option<&u8>| c.and_then(|c| (*c as char).to_digit(16));
                match (hex(b.get(i + 1)), hex(b.get(i + 2))) {
                    (Some(h), Some(l)) => {
                        out.push((h * 16 + l) as u8);
                        i += 3;
                    }
                    _ => return String::new(),
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn extract_adm_native(ad_type: i32, content: &Content, bid_type: BidType, imp: &Imp) -> AdmResult {
    if ad_type != NATIVE {
        return Err(plain("extract Adm for Native ad: huaweiads response is not a native ad"));
    }
    let Some(native) = imp.native.as_ref() else {
        return Err(plain("extract Adm for Native ad: imp.Native is nil"));
    };
    if native.request.is_empty() {
        return Err(plain("extract Adm for Native ad: imp.Native.Request is empty"));
    }
    let native_payload: NativeRequest = jsonutil::unmarshal(native.request.as_bytes())?;

    let mut native_result = NativeResponse::default();
    let mut link_object = NativeLink::default();
    link_object.url = get_click_url(content)?;

    let mut ad_width = 0i64;
    let mut ad_height = 0i64;
    native_result.assets = Vec::with_capacity(native_payload.assets.len());
    let mut img_index = 0usize;
    let mut icon_index = 0usize;
    for asset in &native_payload.assets {
        let mut response_asset = NativeAsset::default();
        if asset.title.is_some() {
            let text = get_decode_value(&content.meta_data.title);
            response_asset.title = Some(NativeTitle { len: text.len() as i64, text, ext: None });
        } else if asset.video.is_some() {
            let (vast, w, h) = extract_adm_video(ad_type, content, bid_type, imp)?;
            ad_width = w;
            ad_height = h;
            response_asset.video = Some(NativeVideo { vasttag: vast });
        } else if let Some(img) = asset.img.as_ref() {
            if content.meta_data.image_info.len() == img_index && img.r#type == ImageAssetType::MAIN {
                continue;
            }
            let mut img_object = NativeImage { r#type: img.r#type, ..Default::default() };
            if img.r#type == ImageAssetType::ICON {
                if content.meta_data.icon.len() > icon_index {
                    let ic = &content.meta_data.icon[icon_index];
                    img_object.url = ic.url.clone();
                    img_object.w = ic.width;
                    img_object.h = ic.height;
                    icon_index += 1;
                }
            } else if content.meta_data.image_info.len() > img_index {
                let ii = &content.meta_data.image_info[img_index];
                img_object.url = ii.url.clone();
                img_object.w = ii.width;
                img_object.h = ii.height;
                img_index += 1;
            }
            if ad_height == 0 && ad_width == 0 {
                ad_height = img_object.h;
                ad_width = img_object.w;
            }
            response_asset.img = Some(img_object);
        } else if let Some(data) = asset.data.as_ref() {
            let mut data_object = NativeData::default();
            if data.r#type == DataAssetType::DESC || data.r#type == DataAssetType::DESC2 {
                data_object.label = "desc".into();
                data_object.value = get_decode_value(&content.meta_data.description);
            }
            if data.r#type == DataAssetType::CTA_TEXT {
                data_object.r#type = DataAssetType::CTA_TEXT;
                data_object.value = get_decode_value(&content.meta_data.cta);
            }
            response_asset.data = Some(data_object);
        }
        response_asset.id = Some(asset.id);
        native_result.assets.push(response_asset);
    }

    // dsp imp click tracking + imp click tracking
    let mut event_trackers = Vec::new();
    for monitor in &content.monitor {
        if monitor.url.is_empty() {
            continue;
        }
        if monitor.event_type == "click" {
            link_object.clicktrackers.extend(monitor.url.iter().cloned());
        }
        if monitor.event_type == "imp" {
            for u in &monitor.url {
                event_trackers.push(NativeEventTracker {
                    event: EventType::IMPRESSION,
                    method: EventTrackingMethod::IMAGE,
                    url: u.clone(),
                    ..Default::default()
                });
            }
        }
    }
    native_result.eventtrackers = event_trackers;
    native_result.link = link_object;
    native_result.ver = "1.1".into();
    if !native_payload.ver.is_empty() {
        native_result.ver = native_payload.ver.clone();
    }

    // json.Encoder with SetEscapeHTML(false): no HTML escaping, but U+2028/U+2029 are escaped.
    let result = serde_json::to_string(&native_result).map_err(|e| plain(e.to_string()))?;
    let result = result.replace('\u{2028}', "\\u2028").replace('\u{2029}', "\\u2029");
    Ok((result.replace('\n', ""), ad_width, ad_height))
}

/// extractAdmPicture: For banner single picture
fn extract_adm_picture(content: &Content) -> AdmResult {
    let click_url = get_click_url(content)?;

    // Go indexes ImageInfo[0] once it is non-nil and panics when it is an empty array; both
    // cases are reported as an error here.
    let Some(first) = content.meta_data.image_info.first() else {
        return Err(plain("content.MetaData.ImageInfo is empty"));
    };
    let image_info_url = first.url.clone();
    let ad_height = first.height;
    let ad_width = first.width;

    let image_title = get_decode_value(&content.meta_data.title);
    // dspImp, Imp, dspClick, Click tracking all can be found in MonitorUrl(imp ,click)
    let (dsp_imp_trackings, dsp_click_trackings) = get_dsp_imp_click_trackings(content);
    let mut imp_img = String::new();
    for t in &dsp_imp_trackings {
        imp_img.push_str("<img height=\"1\" width=\"1\" src='");
        imp_img.push_str(t);
        imp_img.push_str("' >  ");
    }

    let adm = [
        "<style> html, body  ",
        "{ margin: 0; padding: 0; width: 100%; height: 100%; vertical-align: middle; }  ",
        "html  ",
        "{ display: table; }  ",
        "body { display: table-cell; vertical-align: middle; text-align: center; -webkit-text-size-adjust: none; }  ",
        "</style> ",
        "<span class=\"title-link advertiser_label\">", &image_title, "</span> ",
        "<a href='", &click_url, "' style=\"text-decoration:none\" ",
        "onclick=sendGetReq()> ",
        "<img src='", &image_info_url, "' width='", &ad_width.to_string(), "' height='", &ad_height.to_string(), "'/> ",
        "</a> ",
        &imp_img,
        "<script type=\"text/javascript\">",
        "var dspClickTrackings = [", &dsp_click_trackings, "];",
        "function sendGetReq() {",
        "sendSomeGetReq(dspClickTrackings)",
        "}",
        "function sendOneGetReq(url) {",
        "var req = new XMLHttpRequest();",
        "req.open('GET', url, true);",
        "req.send(null);",
        "}",
        "function sendSomeGetReq(urls) {",
        "for (var i = 0; i < urls.length; i++) {",
        "sendOneGetReq(urls[i]);",
        "}",
        "}",
        "</script>",
    ]
    .concat();
    Ok((adm, ad_width, ad_height))
}

/// for Interactiontype == appPromotion, clickUrl is intent
fn get_click_url(content: &Content) -> Result<String, BidderError> {
    let mut click_url = String::new();
    if content.interactiontype == APP_PROMOTION {
        if !content.meta_data.intent.is_empty() {
            click_url = get_decode_value(&content.meta_data.intent);
        } else {
            return Err(plain(
                "content.MetaData.Intent in huaweiads resopnse is empty when interactiontype is appPromotion",
            ));
        }
    } else if !content.meta_data.click_url.is_empty() {
        click_url = content.meta_data.click_url.clone();
    } else if !content.meta_data.intent.is_empty() {
        click_url = get_decode_value(&content.meta_data.intent);
    }
    Ok(click_url)
}

fn get_dsp_imp_click_trackings(content: &Content) -> (Vec<String>, String) {
    let mut imp = Vec::new();
    let mut click = String::new();
    for monitor in &content.monitor {
        if !monitor.url.is_empty() {
            match monitor.event_type.as_str() {
                "imp" => imp = monitor.url.clone(),
                "click" => click = get_strings(&monitor.url),
                _ => {}
            }
        }
    }
    (imp, click)
}

fn get_strings(eles: &[String]) -> String {
    eles.iter().map(|e| format!("\"{e}\"")).collect::<Vec<_>>().join(",")
}

/// getDuration: millisecond -> format: 00:00:00.000 (Go formats `time.Time{}.Add(d)`, so the hour
/// wraps at 24).
fn get_duration(duration: i64) -> String {
    let ms = duration.rem_euclid(86_400_000);
    format!("{:02}:{:02}:{:02}.{:03}", ms / 3_600_000, (ms / 60_000) % 60, (ms / 1000) % 60, ms % 1000)
}

/// extractAdmVideo: get field adm for video, vast 3.0
fn extract_adm_video(ad_type: i32, content: &Content, bid_type: BidType, imp: &Imp) -> AdmResult {
    let click_url = get_click_url(content)?;

    let mut mime = "video/mp4".to_string();
    let resource_url;
    let duration;
    let mut ad_width: i64 = 0;
    let mut ad_height: i64 = 0;
    if ad_type == ROLL {
        // roll ad get information from mediafile
        if !content.meta_data.media_file.mime.is_empty() {
            mime = content.meta_data.media_file.mime.clone();
        }
        ad_width = content.meta_data.media_file.width;
        ad_height = content.meta_data.media_file.height;
        if !content.meta_data.media_file.url.is_empty() {
            resource_url = content.meta_data.media_file.url.clone();
        } else {
            return Err(plain("extract Adm for video failed: Content.MetaData.MediaFile.Url is empty"));
        }
        duration = get_duration(content.meta_data.duration);
    } else {
        if !content.meta_data.video_info.video_download_url.is_empty() {
            resource_url = content.meta_data.video_info.video_download_url.clone();
        } else {
            return Err(plain("extract Adm for video failed: content.MetaData.VideoInfo.VideoDownloadUrl is empty"));
        }
        let vi = &content.meta_data.video_info;
        if vi.width != 0 && vi.height != 0 {
            ad_width = i64::from(vi.width);
            ad_height = i64::from(vi.height);
        } else if bid_type == BidType::Video {
            if let Some(v) = imp.video.as_ref() {
                let (w, h) = (v.w.unwrap_or_default(), v.h.unwrap_or_default());
                if w != 0 && h != 0 {
                    ad_width = w;
                    ad_height = h;
                }
            }
        } else {
            return Err(plain("extract Adm for video failed: cannot get video width, height"));
        }
        duration = get_duration(i64::from(vi.video_duration));
    }

    let ad_title = get_decode_value(&content.meta_data.title);
    let ad_id = content.contentid.clone();
    let creative_id = content.contentid.clone();
    let mut tracking_events = String::new();
    let mut dsp_imp_tracking = String::new();
    let mut dsp_click_tracking = String::new();
    let mut error_tracking = String::new();
    for monitor in &content.monitor {
        if monitor.url.is_empty() {
            continue;
        }
        let mut event = "";
        match monitor.event_type.as_str() {
            "vastError" => error_tracking = get_vast_imp_click_error_tracking_urls(&monitor.url, "vastError"),
            "imp" => dsp_imp_tracking = get_vast_imp_click_error_tracking_urls(&monitor.url, "imp"),
            "click" => dsp_click_tracking = get_vast_imp_click_error_tracking_urls(&monitor.url, "click"),
            "userclose" => event = "skip&closeLinear",
            "playStart" => event = "start",
            "playEnd" => event = "complete",
            "playResume" => event = "resume",
            "playPause" => event = "pause",
            "soundClickOff" => event = "mute",
            "soundClickOn" => event = "unmute",
            _ => {}
        }
        if !event.is_empty() {
            tracking_events.push_str(&get_vast_event_tracking_urls(&monitor.url, event));
        }
    }

    // Only for rewarded video
    let mut rewarded_video_part = String::new();
    if ad_type == REWARDED {
        let static_image_type = "image/png";
        let static_image_url;
        let static_image_height;
        let static_image_width;
        let mut add_part = true;
        let icon = content.meta_data.icon.first().filter(|i| !i.url.is_empty());
        let img = content.meta_data.image_info.first().filter(|i| !i.url.is_empty());
        if let Some(ic) = icon {
            static_image_url = ic.url.clone();
            if ic.height > 0 && ic.width > 0 {
                static_image_height = ic.height.to_string();
                static_image_width = ic.width.to_string();
            } else {
                static_image_height = ad_height.to_string();
                static_image_width = ad_width.to_string();
            }
        } else if let Some(ii) = img {
            static_image_url = ii.url.clone();
            if ii.height > 0 && ii.width > 0 {
                static_image_height = ii.height.to_string();
                static_image_width = ii.width.to_string();
            } else {
                static_image_height = ad_height.to_string();
                static_image_width = ad_width.to_string();
            }
        } else {
            add_part = false;
            static_image_url = String::new();
            static_image_height = String::new();
            static_image_width = String::new();
        }
        if add_part {
            rewarded_video_part = [
                "<Creative adId=\"", &ad_id, "\" id=\"", &creative_id, "\">",
                "<CompanionAds>",
                "<Companion width=\"", &static_image_width, "\" height=\"", &static_image_height, "\">",
                "<StaticResource creativeType=\"", static_image_type, "\"><![CDATA[", &static_image_url, "]]></StaticResource>",
                "<CompanionClickThrough><![CDATA[", &click_url, "]]></CompanionClickThrough>",
                "</Companion>",
                "</CompanionAds>",
                "</Creative>",
            ]
            .concat();
        }
    }

    let adm = [
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
        "<VAST version=\"3.0\">",
        "<Ad id=\"", &ad_id, "\"><InLine>",
        "<AdSystem>HuaweiAds</AdSystem>",
        "<AdTitle>", &ad_title, "</AdTitle>",
        &error_tracking, &dsp_imp_tracking,
        "<Creatives>",
        "<Creative adId=\"", &ad_id, "\" id=\"", &creative_id, "\">",
        "<Linear>",
        "<Duration>", &duration, "</Duration>",
        "<TrackingEvents>", &tracking_events, "</TrackingEvents>",
        "<VideoClicks>",
        "<ClickThrough><![CDATA[", &click_url, "]]></ClickThrough>",
        &dsp_click_tracking,
        "</VideoClicks>",
        "<MediaFiles>",
        "<MediaFile delivery=\"progressive\" type=\"", &mime, "\" width=\"", &ad_width.to_string(), "\" ",
        "height=\"", &ad_height.to_string(), "\" scalable=\"true\" maintainAspectRatio=\"true\"> ",
        "<![CDATA[", &resource_url, "]]>",
        "</MediaFile>",
        "</MediaFiles>",
        "</Linear>",
        "</Creative>", &rewarded_video_part,
        "</Creatives>",
        "</InLine>",
        "</Ad>",
        "</VAST>",
    ]
    .concat();
    Ok((adm, ad_width, ad_height))
}

fn get_vast_imp_click_error_tracking_urls(urls: &[String], event_type: &str) -> String {
    let mut out = String::new();
    for url in urls {
        match event_type {
            "click" => {
                out.push_str("<ClickTracking><![CDATA[");
                out.push_str(url);
                out.push_str("]]></ClickTracking>");
            }
            "imp" => {
                out.push_str("<Impression><![CDATA[");
                out.push_str(url);
                out.push_str("]]></Impression>");
            }
            "vastError" => {
                out.push_str("<Error><![CDATA[");
                out.push_str(url);
                out.push_str("&et=[ERRORCODE]]]></Error>");
            }
            _ => {}
        }
    }
    out
}

fn get_vast_event_tracking_urls(urls: &[String], event_type: &str) -> String {
    let mut out = String::new();
    for event_url in urls {
        if event_type == "skip&closeLinear" {
            out.push_str("<Tracking event=\"skip\"><![CDATA[");
            out.push_str(event_url);
            out.push_str("]]></Tracking><Tracking event=\"closeLinear\"><![CDATA[");
            out.push_str(event_url);
            out.push_str("]]></Tracking>");
        } else {
            out.push_str("<Tracking event=\"");
            out.push_str(event_type);
            out.push_str("\"><![CDATA[");
            out.push_str(event_url);
            out.push_str("]]></Tracking>");
        }
    }
    out
}

// ---- HMAC-SHA256 (Go crypto/hmac + crypto/sha256) ----

fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98,
        0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786,
        0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8,
        0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
        0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819,
        0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a,
        0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] =
        [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([chunk[4 * i], chunk[4 * i + 1], chunk[4 * i + 2], chunk[4 * i + 3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (slot, v) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *slot = slot.wrapping_add(v);
        }
    }
    let mut out = [0u8; 32];
    for (i, v) in h.iter().enumerate() {
        out[4 * i..4 * i + 4].copy_from_slice(&v.to_be_bytes());
    }
    out
}

fn compute_hmac_sha256(message: &str, sign_key: &str) -> String {
    let mut key = sign_key.as_bytes().to_vec();
    if key.len() > 64 {
        key = sha256(&key).to_vec();
    }
    key.resize(64, 0);
    let mut inner: Vec<u8> = key.iter().map(|b| b ^ 0x36).collect();
    inner.extend_from_slice(message.as_bytes());
    let inner_hash = sha256(&inner);
    let mut outer: Vec<u8> = key.iter().map(|b| b ^ 0x5c).collect();
    outer.extend_from_slice(&inner_hash);
    sha256(&outer).iter().map(|b| format!("{b:02x}")).collect()
}

/// getDigestAuthorization: get digest authorization for request header
fn get_digest_authorization(ext: &ExtImpHuaweiAds, is_test_authorization: bool) -> String {
    let mut nonce = now_millis().to_string();
    // this is for test case, time 2021/8/20 19:30
    if is_test_authorization {
        nonce = "1629473330823".to_string();
    }
    let publisher_id = ext.publisher_id.trim();
    let sign_key = ext.sign_key.trim();
    let key_id = ext.key_id.trim();
    let api_key = format!("{publisher_id}:ppsadx/getResult:{sign_key}");
    format!(
        "Digest username={publisher_id},realm=ppsadx/getResult,nonce={nonce},response={},algorithm=HmacSHA256,usertype=1,keyid={key_id}",
        compute_hmac_sha256(&format!("{nonce}:POST:/ppsadx/getResult"), &api_key)
    )
}

// ---- mcc_list.go ----
const MCC_LIST: &[(i64, &str)] = &[
    (202, "gr"),
    (204, "nl"),
    (206, "be"),
    (208, "fr"),
    (212, "mc"),
    (213, "ad"),
    (214, "es"),
    (216, "hu"),
    (218, "ba"),
    (219, "hr"),
    (220, "rs"),
    (222, "it"),
    (225, "va"),
    (226, "ro"),
    (228, "ch"),
    (230, "cz"),
    (231, "sk"),
    (232, "at"),
    (234, "gb"),
    (235, "gb"),
    (238, "dk"),
    (240, "se"),
    (242, "no"),
    (244, "fi"),
    (246, "lt"),
    (247, "lv"),
    (248, "ee"),
    (250, "ru"),
    (255, "ua"),
    (257, "by"),
    (259, "md"),
    (260, "pl"),
    (262, "de"),
    (266, "gi"),
    (268, "pt"),
    (270, "lu"),
    (272, "ie"),
    (274, "is"),
    (276, "al"),
    (278, "mt"),
    (280, "cy"),
    (282, "ge"),
    (283, "am"),
    (284, "bg"),
    (286, "tr"),
    (288, "fo"),
    (289, "ge"),
    (290, "gl"),
    (292, "sm"),
    (293, "si"),
    (294, "mk"),
    (295, "li"),
    (297, "me"),
    (302, "ca"),
    (308, "pm"),
    (310, "us"),
    (311, "us"),
    (312, "us"),
    (313, "us"),
    (314, "us"),
    (315, "us"),
    (316, "us"),
    (330, "pr"),
    (332, "vi"),
    (334, "mx"),
    (338, "jm"),
    (340, "gp"),
    (342, "bb"),
    (344, "ag"),
    (346, "ky"),
    (348, "vg"),
    (350, "bm"),
    (352, "gd"),
    (354, "ms"),
    (356, "kn"),
    (358, "lc"),
    (360, "vc"),
    (362, "ai"),
    (363, "aw"),
    (364, "bs"),
    (365, "ai"),
    (366, "dm"),
    (368, "cu"),
    (370, "do"),
    (372, "ht"),
    (374, "tt"),
    (376, "tc"),
    (400, "az"),
    (401, "kz"),
    (402, "bt"),
    (404, "in"),
    (405, "in"),
    (406, "in"),
    (410, "pk"),
    (412, "af"),
    (413, "lk"),
    (414, "mm"),
    (415, "lb"),
    (416, "jo"),
    (417, "sy"),
    (418, "iq"),
    (419, "kw"),
    (420, "sa"),
    (421, "ye"),
    (422, "om"),
    (423, "ps"),
    (424, "ae"),
    (425, "il"),
    (426, "bh"),
    (427, "qa"),
    (428, "mn"),
    (429, "np"),
    (430, "ae"),
    (431, "ae"),
    (432, "ir"),
    (434, "uz"),
    (436, "tj"),
    (437, "kg"),
    (438, "tm"),
    (440, "jp"),
    (441, "jp"),
    (450, "kr"),
    (452, "vn"),
    (454, "hk"),
    (455, "mo"),
    (456, "kh"),
    (457, "la"),
    (460, "cn"),
    (461, "cn"),
    (466, "tw"),
    (467, "kp"),
    (470, "bd"),
    (472, "mv"),
    (502, "my"),
    (505, "au"),
    (510, "id"),
    (514, "tl"),
    (515, "ph"),
    (520, "th"),
    (525, "sg"),
    (528, "bn"),
    (530, "nz"),
    (534, "mp"),
    (535, "gu"),
    (536, "nr"),
    (537, "pg"),
    (539, "to"),
    (540, "sb"),
    (541, "vu"),
    (542, "fj"),
    (543, "wf"),
    (544, "as"),
    (545, "ki"),
    (546, "nc"),
    (547, "pf"),
    (548, "ck"),
    (549, "ws"),
    (550, "fm"),
    (551, "mh"),
    (552, "pw"),
    (553, "tv"),
    (555, "nu"),
    (602, "eg"),
    (603, "dz"),
    (604, "ma"),
    (605, "tn"),
    (606, "ly"),
    (607, "gm"),
    (608, "sn"),
    (609, "mr"),
    (610, "ml"),
    (611, "gn"),
    (612, "ci"),
    (613, "bf"),
    (614, "ne"),
    (615, "tg"),
    (616, "bj"),
    (617, "mu"),
    (618, "lr"),
    (619, "sl"),
    (620, "gh"),
    (621, "ng"),
    (622, "td"),
    (623, "cf"),
    (624, "cm"),
    (625, "cv"),
    (626, "st"),
    (627, "gq"),
    (628, "ga"),
    (629, "cg"),
    (630, "cg"),
    (631, "ao"),
    (632, "gw"),
    (633, "sc"),
    (634, "sd"),
    (635, "rw"),
    (636, "et"),
    (637, "so"),
    (638, "dj"),
    (639, "ke"),
    (640, "tz"),
    (641, "ug"),
    (642, "bi"),
    (643, "mz"),
    (645, "zm"),
    (646, "mg"),
    (647, "re"),
    (648, "zw"),
    (649, "na"),
    (650, "mw"),
    (651, "ls"),
    (652, "bw"),
    (653, "sz"),
    (654, "km"),
    (655, "za"),
    (657, "er"),
    (658, "sh"),
    (659, "ss"),
    (702, "bz"),
    (704, "gt"),
    (706, "sv"),
    (708, "hn"),
    (710, "ni"),
    (712, "cr"),
    (714, "pa"),
    (716, "pe"),
    (722, "ar"),
    (724, "br"),
    (730, "cl"),
    (732, "co"),
    (734, "ve"),
    (736, "bo"),
    (738, "gy"),
    (740, "ec"),
    (742, "gf"),
    (744, "py"),
    (746, "sr"),
    (748, "uy"),
    (750, "fk"),
];
