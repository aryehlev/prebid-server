//! Go `adapters/silverpush/silverpush.go` and `devicetype.go`.

use std::sync::OnceLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::bid_types::BidType;
use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder,
    BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::adcom1::DeviceType;
use crate::ortb::openrtb2::{Banner, Bid, BidRequest, BidResponse, Eid, Imp, MarkupType, Publisher, User, Video};
use crate::ortb::Ext;

const BIDDER_CONFIG: &str = "sp_pb_ortb";
const BIDDER_VERSION: &str = "1.0.0";

pub struct Adapter {
    endpoint: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self { endpoint: endpoint.into() }
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpBidder {
    bidder: Option<Ext>,
}

/// Go `openrtb_ext.ImpExtSilverpush`.
#[derive(Deserialize, Default)]
#[serde(default)]
struct ImpExtSilverpush {
    #[serde(rename = "publisherId")]
    publisher_id: String,
    bidfloor: f64,
}

/// The part of Go `openrtb_ext.ExtUser` this adapter reads.
#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtUser {
    eids: Vec<Eid>,
}

fn ext_text(ext: &Option<Ext>) -> Vec<u8> {
    ext.as_ref().map(|e| e.to_json().into_bytes()).unwrap_or_default()
}

// devicetype.go
fn is_mobile(ua: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(ios|ipod|ipad|iphone|android)").unwrap()).is_match(&ua.to_lowercase())
}

fn is_ctv(ua: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(smart[-]?tv|hbbtv|appletv|googletv|hdmi|netcast\.tv|viera|nettv|roku|(?-u:\b)dtv(?-u:\b)|sonydtv|inettvbrowser|(?-u:\b)tv(?-u:\b))").unwrap()
    })
    .is_match(&ua.to_lowercase())
}

fn is_valid_eids(eids: &[Eid]) -> bool {
    eids.iter().any(|e| e.uids.first().is_some_and(|u| !u.id.is_empty()))
}

fn get_os(ua: &str) -> &'static str {
    static IOS: OnceLock<Regex> = OnceLock::new();
    if ua.contains("Windows") {
        "Windows"
    } else if IOS.get_or_init(|| Regex::new(r"(iPhone|iPod|iPad)").unwrap()).is_match(ua) {
        "iOS"
    } else if ua.contains("Mac OS X") {
        "macOS"
    } else if ua.contains("Android") {
        "Android"
    } else if ua.contains("Linux") {
        "Linux"
    } else {
        "Unknown"
    }
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request_in: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        // Go mutates the shared request across imps (site, user, device, ext persist), so one
        // working copy is mutated in sequence.
        let mut request = request_in.clone();
        let mut errors = vec![];
        let mut requests = Vec::with_capacity(request_in.imp.len());
        for imp in &request_in.imp {
            request.imp = vec![impression_by_media_type(imp)];
            if let Err(e) = validate_request(&mut request) {
                errors.push(e);
                continue;
            }
            match self.make_request(&request) {
                Ok(r) => requests.push(r),
                Err(e) => errors.push(e),
            }
        }
        (requests, errors)
    }

    fn make_bids(
        &self,
        internal_request: &BidRequest,
        _external_request: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if is_response_status_code_no_content(response) {
            return (None, vec![]);
        }
        if check_response_status_code_for_errors(response).is_some() {
            return (
                None,
                vec![BidderError::bad_input(format!(
                    "Unexpected status code: {}. Run with request.debug = 1 for more info",
                    response.status_code
                ))],
            );
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut bid_response = BidderResponse::with_bids_capacity(internal_request.imp.len());
        if !bid_resp.cur.is_empty() {
            bid_response.currency = bid_resp.cur.clone();
        }
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                // Go's `getMediaTypeForImp` returns "" for other mtypes; the typed `BidType`
                // cannot hold that, so such a bid is reported instead of passed on untyped.
                match get_media_type_for_imp(&bid) {
                    Some(t) => bid_response.bids.push(TypedBid::new(bid, t)),
                    None => {
                        return (
                            None,
                            vec![BidderError::bad_server_response(format!(
                                "Unsupported return mType: {}",
                                bid.mtype.0
                            ))],
                        )
                    }
                }
            }
        }
        (Some(bid_response), vec![])
    }
}

impl Adapter {
    fn make_request(&self, req: &BidRequest) -> Result<RequestData, BidderError> {
        let body = crate::go_json::to_vec(req).map_err(|e| BidderError::other(e.to_string()))?;
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        headers.add("X-Openrtb-Version", "2.5");
        Ok(RequestData {
            method: "POST".into(),
            uri: self.endpoint.clone(),
            body,
            headers,
            imp_ids: req.imp.iter().map(|i| i.id.clone()).collect(),
        })
    }
}

fn impression_by_media_type(imp: &Imp) -> Imp {
    let mut copy = imp.clone();
    if imp.banner.is_some() {
        copy.video = None;
    }
    if imp.video.is_some() {
        copy.banner = None;
    }
    copy
}

fn validate_request(req: &mut BidRequest) -> Result<(), BidderError> {
    let mut silver_push_ext = ImpExtSilverpush::default();
    set_publisher_id(req, &mut silver_push_ext)?;
    set_user(req)?;
    set_device(req);
    set_ext_to_request(req, &silver_push_ext.publisher_id)?;
    set_imp_for_ad_exchange(&mut req.imp[0], &silver_push_ext)
}

fn set_device(req: &mut BidRequest) {
    let Some(device) = req.device.as_mut() else { return };
    if device.ua.is_empty() {
        return;
    }
    device.os = get_os(&device.ua).to_string();
    device.devicetype = if is_mobile(&device.ua) {
        DeviceType(1)
    } else if is_ctv(&device.ua) {
        DeviceType(3)
    } else {
        DeviceType(2)
    };
}

fn set_user(req: &mut BidRequest) -> Result<(), BidderError> {
    let Some(user) = req.user.as_mut() else { return Ok(()) };
    let Some(ext) = user.ext.as_ref() else { return Ok(()) };
    let user_ext_raw: serde_json::Map<String, serde_json::Value> =
        jsonutil::unmarshal(ext.to_json().as_bytes()).map_err(|_| BidderError::bad_input("Invalid user.ext."))?;
    if let Some(data) = user_ext_raw.get("data") {
        let ext_user: ExtUser = jsonutil::unmarshal(data.to_string().as_bytes())
            .map_err(|_| BidderError::bad_input("Invalid user.ext.data."))?;
        if is_valid_eids(&ext_user.eids) {
            let new_ext = Ext::from_serialize(&User { eids: ext_user.eids, ..Default::default() })
                .map_err(|_| BidderError::bad_input("Error in marshaling user.eids."))?;
            user.ext = Some(new_ext);
        }
    }
    Ok(())
}

fn set_ext_to_request(req: &mut BidRequest, publisher_id: &str) -> Result<(), BidderError> {
    #[derive(Serialize)]
    struct Record<'a> {
        bc: String,
        #[serde(rename = "publisherId")]
        publisher_id: &'a str,
    }
    // Go marshals a `map[string]string`, so the keys come out sorted: `bc`, `publisherId`.
    let record = Record { bc: format!("{BIDDER_CONFIG}_{BIDDER_VERSION}"), publisher_id };
    req.ext = Some(Ext::from_serialize(&record).map_err(|e| BidderError::other(e.to_string()))?);
    Ok(())
}

fn set_imp_for_ad_exchange(imp: &mut Imp, imp_ext: &ImpExtSilverpush) -> Result<(), BidderError> {
    if imp_ext.bidfloor == 0.0 {
        if imp.banner.is_some() {
            imp.bidfloor = 0.05;
        } else if imp.video.is_some() {
            imp.bidfloor = 0.1;
        }
    } else {
        imp.bidfloor = imp_ext.bidfloor;
    }
    if let Some(banner) = imp.banner.as_ref() {
        imp.banner = Some(set_banner_dimension(banner)?);
    }
    if let Some(video) = imp.video.as_ref() {
        imp.video = Some(check_video_dimension(video)?);
    }
    Ok(())
}

/// Go compares `API`, `MIMEs` and `Protocols` with nil; the typed Rust slices cannot tell nil
/// from empty, so empty counts as nil.
fn check_video_dimension(video: &Video) -> Result<Video, BidderError> {
    let mut copy = video.clone();
    if copy.maxduration == 0 {
        copy.maxduration = 120;
    }
    if copy.maxduration < copy.minduration {
        copy.maxduration = copy.minduration;
        copy.minduration = 0;
    }
    if copy.api.is_empty() || copy.mimes.is_none() || copy.protocols.is_empty() || copy.minduration < 0 {
        return Err(BidderError::bad_input("Invalid or missing video field(s)"));
    }
    Ok(copy)
}

fn set_banner_dimension(banner: &Banner) -> Result<Banner, BidderError> {
    if banner.w.is_some() && banner.h.is_some() {
        return Ok(banner.clone());
    }
    let Some(f) = banner.format.first() else {
        return Err(BidderError::bad_input("No sizes provided for Banner."));
    };
    let mut copy = banner.clone();
    copy.w = Some(f.w);
    copy.h = Some(f.h);
    Ok(copy)
}

fn set_publisher_id(req: &mut BidRequest, imp_ext: &mut ImpExtSilverpush) -> Result<(), BidderError> {
    let bidder_ext: ExtImpBidder = jsonutil::unmarshal(&ext_text(&req.imp[0].ext))
        .map_err(|e| BidderError::bad_input(e.to_string()))?;
    *imp_ext = jsonutil::unmarshal(&ext_text(&bidder_ext.bidder))
        .map_err(|e| BidderError::bad_input(e.to_string()))?;
    if imp_ext.publisher_id.is_empty() {
        return Err(BidderError::bad_input("Missing publisherId parameter."));
    }
    if let Some(site) = req.site.as_mut() {
        match site.publisher.as_mut() {
            None => site.publisher = Some(Publisher { id: imp_ext.publisher_id.clone(), ..Default::default() }),
            Some(p) => p.id = imp_ext.publisher_id.clone(),
        }
    } else if let Some(app) = req.app.as_mut() {
        // Go sets the id on a copy of the publisher and then replaces it with a bare publisher.
        app.publisher = Some(Publisher { id: imp_ext.publisher_id.clone(), ..Default::default() });
    }
    Ok(())
}

fn get_media_type_for_imp(bid: &Bid) -> Option<BidType> {
    match bid.mtype {
        MarkupType::BANNER => Some(BidType::Banner),
        MarkupType::VIDEO => Some(BidType::Video),
        _ => None,
    }
}
