//! Go `adapters/smaato` (`smaato.go`, `banner.go`, `native.go`).

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use sonic_rs::{JsonContainerTrait, JsonValueTrait};

use crate::bid_types::{BidType, ExtBidPrebidVideo};
use crate::bidder::{
    is_response_status_code_no_content, Bidder, BidderResponse, ExtraRequestInfo, RequestData,
    ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::ext_helpers::{ext_remove, ext_str};
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{Bid, BidRequest, BidResponse, Imp, Publisher};
use crate::ortb::Ext;

const CLIENT_VERSION: &str = "prebid_server_1.2";

const SMT_AD_TYPE_IMG: &str = "Img";
const SMT_AD_TYPE_RICHMEDIA: &str = "Richmedia";
const SMT_AD_TYPE_VIDEO: &str = "Video";
const SMT_AD_TYPE_NATIVE: &str = "Native";

/// Go `timeutil.Time`: current time in milliseconds since the epoch.
type Clock = Box<dyn Fn() -> i64 + Send + Sync>;

pub struct Adapter {
    clock: Clock,
    endpoint: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        let clock: Clock = Box::new(|| {
            SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
        });
        Self { clock, endpoint: endpoint.into() }
    }

    /// Replaces the clock (Go tests swap `adapter.clock` for a `mockTime`).
    pub fn with_clock(mut self, now_millis: impl Fn() -> i64 + Send + Sync + 'static) -> Self {
        self.clock = Box::new(now_millis);
        self
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct UserExtData {
    keywords: String,
    gender: String,
    // Go rejects `"yob": ""` for an int64; the lenient `de::int` would accept it as 0.
    yob: i64,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct SiteExtData {
    keywords: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct SiteExt {
    data: SiteExtData,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct BidExt {
    #[serde(deserialize_with = "crate::ortb::de::int")]
    duration: i32,
    #[serde(deserialize_with = "crate::ortb::de::strings")]
    curls: Vec<String>,
}

/// Go `url.QueryEscape`.
fn query_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Go `extractAdmBanner`.
fn extract_adm_banner(ad_markup: &str, curls: &[String]) -> String {
    if !curls.is_empty() {
        let mut clicks = String::new();
        for c in curls {
            clicks.push_str(&format!(
                "fetch(decodeURIComponent('{}'.replace(/\\+/g, ' ')), {{cache: 'no-cache'}});",
                query_escape(c)
            ));
        }
        let click_event = format!("onclick=\"{clicks}\"");
        return format!("<div style=\"cursor:pointer\" {click_event}>{ad_markup}</div>");
    }
    ad_markup.to_string()
}

/// Go `extractAdmNative`: the `native` member of the markup, re-marshalled by `encoding/json`
/// (a `RawMessage` is compacted with HTML escaping).
fn extract_adm_native(ad_markup: &str) -> Result<String, BidderError> {
    #[derive(Deserialize, Default)]
    #[serde(default)]
    struct NativeAd {
        native: Option<Ext>,
    }
    let invalid = || BidderError::bad_server_response(format!("Invalid ad markup {ad_markup}."));
    let ad: NativeAd = jsonutil::unmarshal(ad_markup.as_bytes()).map_err(|_| invalid())?;
    let raw = match ad.native {
        Some(n) => sonic_rs::to_string(&n.0).map_err(|_| invalid())?,
        None => "null".to_string(),
    };
    Ok(raw
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029"))
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::bad_input("No impressions in bid request.")]);
        }
        // Go mutates the request it is given across the whole call; do the same on a private copy.
        let mut request = request.clone();
        if let Err(e) = prepare_common_request(&mut request) {
            return (vec![], vec![e]);
        }
        if req_info.pbs_entry_point == "video" {
            self.make_pod_requests(&mut request)
        } else {
            self.make_individual_requests(&mut request)
        }
    }

    fn make_bids(
        &self,
        _internal_request: &BidRequest,
        _external_request: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if is_response_status_code_no_content(response) {
            return (None, vec![]);
        }
        if response.status_code != 200 {
            return (
                None,
                vec![BidderError::bad_server_response(format!(
                    "Unexpected status code: {}. Run with request.debug = 1 for more info.",
                    response.status_code
                ))],
            );
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut bid_response = BidderResponse::with_bids_capacity(5);

        let ad_type = response.headers.get("X-Smt-Adtype");
        if ad_type.is_empty() {
            return (None, vec![BidderError::bad_server_response("X-Smt-Adtype header is missing.")]);
        }

        let mut errors = Vec::new();
        for seat_bid in bid_resp.seatbid {
            for mut bid in seat_bid.bid {
                let bid_ext = match extract_bid_ext(&bid) {
                    Ok(e) => e,
                    Err(e) => {
                        errors.push(e);
                        continue;
                    }
                };
                match render_ad_markup(ad_type, &bid_ext, &bid) {
                    Ok(adm) => bid.adm = adm,
                    Err(e) => {
                        // Go assigns the empty string before checking the error, then skips the bid.
                        errors.push(e);
                        continue;
                    }
                }
                let bid_type = match convert_ad_markup_type_to_media_type(ad_type) {
                    Ok(t) => t,
                    Err(e) => {
                        errors.push(e);
                        continue;
                    }
                };
                let bid_video = build_bid_video(&bid, &bid_ext, bid_type);
                bid.exp = self.get_ttl_from_header_or_default(response);
                let mut typed = TypedBid::new(bid, bid_type);
                typed.bid_video = bid_video;
                bid_response.bids.push(typed);
            }
        }
        (Some(bid_response), errors)
    }
}

impl Adapter {
    fn make_individual_requests(&self, request: &mut BidRequest) -> (Vec<RequestData>, Vec<BidderError>) {
        let imps = request.imp.clone();
        let mut requests = Vec::with_capacity(imps.len());
        let mut errors = Vec::with_capacity(imps.len());

        for imp in imps {
            let by_media_type = match split_impressions_by_media_type(imp) {
                Ok(i) => i,
                Err(e) => {
                    errors.push(e);
                    continue;
                }
            };
            for imp_by_media_type in by_media_type {
                request.imp = vec![imp_by_media_type];
                if let Err(e) = prepare_individual_request(request) {
                    errors.push(e);
                    continue;
                }
                match self.make_request(request) {
                    Ok(r) => requests.push(r),
                    Err(e) => errors.push(e),
                }
            }
        }
        (requests, errors)
    }

    fn make_pod_requests(&self, request: &mut BidRequest) -> (Vec<RequestData>, Vec<BidderError>) {
        let (mut pods, ordered_keys, mut errors) = group_impressions_by_pod(&request.imp);
        let mut requests = Vec::with_capacity(pods.len());

        for key in ordered_keys {
            request.imp = pods.remove(&key).unwrap_or_default();
            if let Err(e) = prepare_pod_request(request) {
                errors.push(e);
                continue;
            }
            match self.make_request(request) {
                Ok(r) => requests.push(r),
                Err(e) => errors.push(e),
            }
        }
        (requests, errors)
    }

    fn make_request(&self, request: &BidRequest) -> Result<RequestData, BidderError> {
        let body = crate::go_json::to_vec(request).map_err(|e| BidderError::other(e.to_string()))?;
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        Ok(RequestData {
            method: "POST".into(),
            uri: self.endpoint.clone(),
            body,
            headers,
            imp_ids: request.imp.iter().map(|i| i.id.clone()).collect(),
        })
    }

    fn get_ttl_from_header_or_default(&self, response: &ResponseData) -> i64 {
        let mut ttl: i64 = 300;
        if let Ok(expires_at_millis) = response.headers.get("X-Smt-Expires").parse::<i64>() {
            let now_millis = (self.clock)();
            ttl = (expires_at_millis - now_millis) / 1000;
            if ttl < 0 {
                ttl = 0;
            }
        }
        ttl
    }
}

fn split_impressions_by_media_type(imp: Imp) -> Result<Vec<Imp>, BidderError> {
    if imp.banner.is_none() && imp.video.is_none() && imp.native.is_none() {
        return Err(BidderError::bad_input("Invalid MediaType. Smaato only supports Banner, Video and Native."));
    }
    let mut imps = Vec::with_capacity(3);
    if imp.banner.is_some() {
        let mut c = imp.clone();
        c.video = None;
        c.native = None;
        imps.push(c);
    }
    if imp.video.is_some() {
        let mut c = imp.clone();
        c.banner = None;
        c.native = None;
        imps.push(c);
    }
    if imp.native.is_some() {
        let mut c = imp;
        c.banner = None;
        c.video = None;
        imps.push(c);
    }
    Ok(imps)
}

fn group_impressions_by_pod(imps: &[Imp]) -> (BTreeMap<String, Vec<Imp>>, Vec<String>, Vec<BidderError>) {
    let mut pods: BTreeMap<String, Vec<Imp>> = BTreeMap::new();
    let mut order = Vec::new();
    let mut errors = Vec::with_capacity(imps.len());
    for imp in imps {
        if imp.video.is_none() {
            errors.push(BidderError::bad_input("Invalid MediaType. Smaato only supports Video for AdPod."));
            continue;
        }
        let pod = imp.id.split('_').next().unwrap_or("").to_string();
        if !pods.contains_key(&pod) {
            order.push(pod.clone());
        }
        pods.entry(pod).or_default().push(imp.clone());
    }
    (pods, order, errors)
}

fn render_ad_markup(ad_type: &str, bid_ext: &BidExt, bid: &Bid) -> Result<String, BidderError> {
    match ad_type {
        SMT_AD_TYPE_IMG | SMT_AD_TYPE_RICHMEDIA => Ok(extract_adm_banner(&bid.adm, &bid_ext.curls)),
        SMT_AD_TYPE_VIDEO => Ok(bid.adm.clone()),
        SMT_AD_TYPE_NATIVE => extract_adm_native(&bid.adm),
        _ => Err(BidderError::bad_server_response(format!("Unknown markup type {ad_type}."))),
    }
}

fn convert_ad_markup_type_to_media_type(ad_type: &str) -> Result<BidType, BidderError> {
    match ad_type {
        SMT_AD_TYPE_IMG | SMT_AD_TYPE_RICHMEDIA => Ok(BidType::Banner),
        SMT_AD_TYPE_VIDEO => Ok(BidType::Video),
        SMT_AD_TYPE_NATIVE => Ok(BidType::Native),
        _ => Err(BidderError::bad_server_response(format!("Unknown markup type {ad_type}."))),
    }
}

fn prepare_common_request(request: &mut BidRequest) -> Result<(), BidderError> {
    set_user(request)?;
    set_site(request)?;
    // `setApp` / `setDOOH` only copy the structs in Go.
    set_ext(request)
}

fn prepare_individual_request(request: &mut BidRequest) -> Result<(), BidderError> {
    set_publisher_id(request, 0)?;
    set_imp_for_adspace(&mut request.imp[0])
}

fn prepare_pod_request(request: &mut BidRequest) -> Result<(), BidderError> {
    if request.imp.is_empty() {
        return Err(BidderError::bad_input("No impressions in bid request."));
    }
    set_publisher_id(request, 0)?;
    set_imp_for_ad_break(&mut request.imp)
}

fn set_user(request: &mut BidRequest) -> Result<(), BidderError> {
    let Some(user) = request.user.as_mut() else { return Ok(()) };
    let Some(ext) = user.ext.as_ref() else { return Ok(()) };

    let invalid = || BidderError::bad_input("Invalid user.ext.");
    if !ext.0.is_object() {
        return Err(invalid());
    }
    let mut raw: BTreeMap<String, Box<serde_json::value::RawValue>> =
        serde_json::from_str(&ext.to_json()).map_err(|_| invalid())?;

    if let Some(data_raw) = raw.get("data") {
        let data: UserExtData = if data_raw.get() == "null" {
            UserExtData::default()
        } else {
            serde_json::from_str(data_raw.get()).map_err(|_| BidderError::bad_input("Invalid user.ext.data."))?
        };
        if !data.gender.is_empty() {
            user.gender = data.gender;
        }
        if data.yob != 0 {
            user.yob = data.yob;
        }
        if !data.keywords.is_empty() {
            user.keywords = data.keywords;
        }
        raw.remove("data");
        let json = serde_json::to_string(&raw).map_err(|e| BidderError::other(e.to_string()))?;
        user.ext = Some(Ext::from_slice(json.as_bytes()).map_err(|e| BidderError::other(e.to_string()))?);
    }
    Ok(())
}

fn set_ext(request: &mut BidRequest) -> Result<(), BidderError> {
    request.ext = Some(
        Ext::from_slice(format!("{{\"client\":\"{CLIENT_VERSION}\"}}").as_bytes())
            .map_err(|e| BidderError::other(e.to_string()))?,
    );
    Ok(())
}

fn set_site(request: &mut BidRequest) -> Result<(), BidderError> {
    if let Some(site) = request.site.as_mut() {
        if let Some(ext) = site.ext.as_ref() {
            let invalid = || BidderError::bad_input("Invalid site.ext.");
            if !ext.0.is_object() {
                return Err(invalid());
            }
            let site_ext: SiteExt = ext.decode().map_err(|_| invalid())?;
            site.keywords = site_ext.data.keywords;
            site.ext = None;
        }
    }
    Ok(())
}

/// Go `jsonparser.GetString(imp.Ext, path...)`: the value must exist and be a string.
fn get_string(imp: &Imp, path: &[&str]) -> Option<String> {
    ext_str(imp.ext.as_ref(), path).map(str::to_string)
}

fn set_publisher_id(request: &mut BidRequest, imp_idx: usize) -> Result<(), BidderError> {
    let Some(publisher_id) = get_string(&request.imp[imp_idx], &["bidder", "publisherId"]) else {
        return Err(BidderError::bad_input("Missing publisherId parameter."));
    };
    let publisher = Publisher { id: publisher_id, ..Default::default() };
    if let Some(site) = request.site.as_mut() {
        site.publisher = Some(publisher);
        Ok(())
    } else if let Some(app) = request.app.as_mut() {
        app.publisher = Some(publisher);
        Ok(())
    } else if let Some(dooh) = request.dooh.as_mut() {
        dooh.publisher = Some(publisher);
        Ok(())
    } else {
        Err(BidderError::bad_input("Missing Site/App/DOOH."))
    }
}

fn set_imp_for_adspace(imp: &mut Imp) -> Result<(), BidderError> {
    let Some(ad_space_id) = get_string(imp, &["bidder", "adspaceId"]) else {
        return Err(BidderError::bad_input("Missing adspaceId parameter."));
    };
    remove_bidder_node_from_imp_ext(imp)?;
    if imp.banner.is_some() || imp.video.is_some() || imp.native.is_some() {
        imp.tagid = ad_space_id;
    }
    Ok(())
}

fn set_imp_for_ad_break(imps: &mut [Imp]) -> Result<(), BidderError> {
    if imps.is_empty() {
        return Err(BidderError::bad_input("No impressions in bid request."));
    }
    let mut first_imp = imps[0].clone();
    let Some(ad_break_id) = get_string(&first_imp, &["bidder", "adbreakId"]) else {
        return Err(BidderError::bad_input("Missing adbreakId parameter."));
    };
    remove_bidder_node_from_imp_ext(&mut first_imp)?;

    for (i, imp) in imps.iter_mut().enumerate() {
        imp.tagid = ad_break_id.clone();
        imp.ext = None;
        // Go dereferences `imps[i].Video`; `group_impressions_by_pod` guarantees it is set.
        if let Some(video) = imp.video.as_mut() {
            video.sequence = (i + 1) as i8;
            video.ext = Ext::from_slice(b"{\"context\":\"adpod\"}").ok();
        }
    }
    imps[0].ext = first_imp.ext;
    Ok(())
}

fn remove_bidder_node_from_imp_ext(imp: &mut Imp) -> Result<(), BidderError> {
    if imp.ext.is_none() {
        return Ok(());
    }
    ext_remove(&mut imp.ext, "bidder").map_err(|e| BidderError::other(e.to_string()))?;
    let is_empty = imp.ext.as_ref().and_then(|e| e.0.as_object()).is_none_or(|o| o.is_empty());
    if is_empty {
        imp.ext = None;
    }
    Ok(())
}

fn build_bid_video(bid: &Bid, bid_ext: &BidExt, bid_type: BidType) -> Option<ExtBidPrebidVideo> {
    if bid_type != BidType::Video {
        return None;
    }
    let primary_category = bid.cat.first().cloned().unwrap_or_default();
    Some(ExtBidPrebidVideo { duration: bid_ext.duration, primary_category })
}

fn extract_bid_ext(bid: &Bid) -> Result<BidExt, BidderError> {
    let Some(ext) = bid.ext.as_ref() else { return Ok(BidExt::default()) };
    let invalid = || BidderError::bad_server_response("Invalid bid.ext.");
    if !ext.0.is_object() {
        return Err(invalid());
    }
    ext.decode().map_err(|_| invalid())
}
