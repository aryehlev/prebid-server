use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use openrtb_ext::BidType;
use serde::Deserialize;

const BIDDER_CONFIG: &str = "sp_pb_ortb";
const BIDDER_VERSION: &str = "1.0.0";

pub struct SilverpushAdapter { pub endpoint: String }
impl SilverpushAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

#[derive(Debug, Deserialize, Default)]
struct ImpExtSilverpush {
    #[serde(rename = "publisherId", default)]
    publisher_id: String,
    #[serde(rename = "bidfloor", default)]
    bid_floor: f64,
}

/// Return the imp restricted to a single media type (banner wins over video per Go logic).
fn impression_by_media_type(imp: &openrtb::Imp) -> openrtb::Imp {
    let mut imp_copy = imp.clone();
    if imp.banner.is_some() {
        imp_copy.video = None;
    } else if imp.video.is_some() {
        imp_copy.banner = None;
    }
    imp_copy
}

/// Detect OS from user agent string.
fn get_os(ua: &str) -> String {
    let ua_lower = ua.to_lowercase();
    if ua_lower.contains("windows") {
        "Windows".to_string()
    } else if ua_lower.contains("android") {
        "Android".to_string()
    } else if ua_lower.contains("iphone") || ua_lower.contains("ipad") || ua_lower.contains("ipod") {
        "iOS".to_string()
    } else if ua_lower.contains("macintosh") || ua_lower.contains("mac os") {
        "macOS".to_string()
    } else if ua_lower.contains("linux") {
        "Linux".to_string()
    } else {
        "".to_string()
    }
}

fn is_mobile(ua: &str) -> bool {
    let ua_lower = ua.to_lowercase();
    ua_lower.contains("mobile") || ua_lower.contains("android") || ua_lower.contains("iphone")
}

fn is_ctv(ua: &str) -> bool {
    let ua_lower = ua.to_lowercase();
    ua_lower.contains("ctv") || ua_lower.contains("appletv") || ua_lower.contains("googletv")
        || ua_lower.contains("firetv") || ua_lower.contains("smarttv") || ua_lower.contains("smart-tv")
        || ua_lower.contains("chromecast")
}

/// Mutate the device field of a request copy: set OS and device type from UA.
fn set_device(req: &mut openrtb::BidRequest) {
    let device = match req.device.as_ref() {
        Some(d) => d,
        None => return,
    };
    let ua = match device.ua.as_ref() {
        Some(ua) if !ua.is_empty() => ua.clone(),
        _ => return,
    };
    let mut device_copy = device.clone();
    device_copy.os = Some(get_os(&ua));
    device_copy.devicetype = Some(
        if is_mobile(&ua) { 1 }
        else if is_ctv(&ua) { 3 }
        else { 2 }
    );
    req.device = Some(device_copy);
}

/// Extract EIDs from user.ext.data and set them directly on user.eids.
fn set_user(req: &mut openrtb::BidRequest) -> Result<(), BidderError> {
    let user = match req.user.as_ref() {
        Some(u) => u,
        None => return Ok(()),
    };
    let ext = match user.ext.as_ref() {
        Some(e) => e,
        None => return Ok(()),
    };

    // Parse user.ext as object to find "data" key
    let ext_map: HashMap<String, serde_json::Value> = match serde_json::from_value(ext.clone()) {
        Ok(m) => m,
        Err(_) => return Err(BidderError::BadInput("Invalid user.ext.".to_string())),
    };

    let data_raw = match ext_map.get("data") {
        Some(d) => d,
        None => return Ok(()),
    };

    // Parse the data value as a user-like object containing eids
    #[derive(Deserialize)]
    struct ExtUserData {
        eids: Option<Vec<openrtb::Eid>>,
    }
    let ext_user: ExtUserData = match serde_json::from_value(data_raw.clone()) {
        Ok(u) => u,
        Err(_) => return Err(BidderError::BadInput("Invalid user.ext.data.".to_string())),
    };

    let eids = match ext_user.eids {
        Some(e) if !e.is_empty() => e,
        _ => return Ok(()),
    };

    // Set user.eids and clear user.ext (replace with object containing only eids)
    let mut user_copy = user.clone();
    user_copy.eids = Some(eids);
    // Replace ext with just the eids serialization (match Go: marshal &openrtb2.User{EIDs: extUser.Eids})
    user_copy.ext = None;
    req.user = Some(user_copy);
    Ok(())
}

/// Set request.ext with bc and publisherId.
fn set_ext_to_request(req: &mut openrtb::BidRequest, publisher_id: &str) -> Result<(), BidderError> {
    let mut record = HashMap::new();
    record.insert("bc".to_string(), format!("{}_{}", BIDDER_CONFIG, BIDDER_VERSION));
    record.insert("publisherId".to_string(), publisher_id.to_string());
    let ext = serde_json::to_value(&record)
        .map_err(|e| BidderError::BadInput(e.to_string()))?;
    req.ext = Some(ext);
    Ok(())
}

/// Parse bidder ext and set publisher ID on site or app.
fn set_publisher_id(req: &mut openrtb::BidRequest, imp: &openrtb::Imp) -> Result<ImpExtSilverpush, BidderError> {
    let bidder_val = imp.ext.as_ref()
        .and_then(|e| e.get("bidder"))
        .cloned()
        .ok_or_else(|| BidderError::BadInput("missing bidder ext".to_string()))?;

    let imp_ext: ImpExtSilverpush = serde_json::from_value(bidder_val)
        .map_err(|e| BidderError::BadInput(e.to_string()))?;

    if imp_ext.publisher_id.is_empty() {
        return Err(BidderError::BadInput("Missing publisherId parameter.".to_string()));
    }

    if let Some(site) = req.site.as_mut() {
        if let Some(pub_) = site.publisher.as_mut() {
            pub_.id = Some(imp_ext.publisher_id.clone());
        } else {
            site.publisher = Some(openrtb::Publisher {
                id: Some(imp_ext.publisher_id.clone()),
                ..Default::default()
            });
        }
    } else if let Some(app) = req.app.as_mut() {
        if let Some(pub_) = app.publisher.as_mut() {
            pub_.id = Some(imp_ext.publisher_id.clone());
        } else {
            app.publisher = Some(openrtb::Publisher {
                id: Some(imp_ext.publisher_id.clone()),
                ..Default::default()
            });
        }
    }

    Ok(imp_ext)
}

/// Set banner dimensions from format if w/h not set.
fn set_banner_dimension(banner: &mut openrtb::Banner) -> Result<(), BidderError> {
    if banner.w.is_some() && banner.h.is_some() {
        return Ok(());
    }
    let formats = banner.format.as_ref()
        .filter(|f| !f.is_empty())
        .ok_or_else(|| BidderError::BadInput("No sizes provided for Banner.".to_string()))?;
    let first = &formats[0];
    banner.w = first.w;
    banner.h = first.h;
    Ok(())
}

/// Validate and fix video dimensions/duration.
fn check_video_dimension(video: &mut openrtb::Video) -> Result<(), BidderError> {
    if video.maxduration.unwrap_or(0) == 0 {
        video.maxduration = Some(120);
    }
    let min = video.minduration.unwrap_or(0);
    let max = video.maxduration.unwrap_or(0);
    if max < min {
        video.maxduration = Some(min);
        video.minduration = Some(0);
    }
    // Validate required fields
    let min_dur = video.minduration.unwrap_or(0);
    if video.api.is_none() || video.mimes.is_none() || video.protocols.is_none() || min_dur < 0 {
        return Err(BidderError::BadInput("Invalid or missing video field(s)".to_string()));
    }
    Ok(())
}

/// Set bid floor and fix imp banner/video dimensions.
fn set_imp_for_ad_exchange(imp: &mut openrtb::Imp, imp_ext: &ImpExtSilverpush) -> Result<(), BidderError> {
    if imp_ext.bid_floor == 0.0 {
        if imp.banner.is_some() {
            imp.bidfloor = Some(0.05);
        } else if imp.video.is_some() {
            imp.bidfloor = Some(0.1);
        }
    } else {
        imp.bidfloor = Some(imp_ext.bid_floor);
    }

    if let Some(banner) = imp.banner.as_mut() {
        set_banner_dimension(banner)?;
    }

    if let Some(video) = imp.video.as_mut() {
        check_video_dimension(video)?;
    }

    Ok(())
}

/// Validate the request: set publisher, user, device, request ext and imp fields.
fn validate_request(req: &mut openrtb::BidRequest) -> Result<(), BidderError> {
    // Clone imp to get ext, then mutate req fields
    let imp_clone = req.imp[0].clone();
    let imp_ext = set_publisher_id(req, &imp_clone)?;
    set_user(req)?;
    set_device(req);
    let publisher_id = imp_ext.publisher_id.clone();
    set_ext_to_request(req, &publisher_id)?;
    set_imp_for_ad_exchange(&mut req.imp[0], &imp_ext)?;
    Ok(())
}

fn get_media_type_for_imp(mtype: i32) -> Option<BidType> {
    match mtype {
        1 => Some(BidType::Banner),
        2 => Some(BidType::Video),
        _ => None,
    }
}

impl Bidder for SilverpushAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();
        let mut errs = Vec::new();

        for imp in &request.imp {
            let filtered_imp = impression_by_media_type(imp);

            let mut req_copy = request.clone();
            req_copy.imp = vec![filtered_imp];

            if let Err(e) = validate_request(&mut req_copy) {
                errs.push(e);
                continue;
            }

            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };
            let mut headers = HashMap::new();
            headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
            headers.insert("Accept".to_string(), "application/json".to_string());
            headers.insert("X-Openrtb-Version".to_string(), "2.5".to_string());
            requests.push(RequestData {
                method: "POST".to_string(),
                uri: self.endpoint.clone(),
                body,
                headers,
                imp_ids: vec![imp.id.clone()],
            });
        }
        (requests, errs)
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(_e) = crate::check_response_status(response.status_code) {
            return Err(vec![BidderError::BadInput(format!("Unexpected status code: {}. Run with request.debug = 1 for more info", response.status_code))]);
        }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(5);
        if let Some(cur) = &bid_resp.cur { if !cur.is_empty() { result.currency = cur.clone(); } }
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let mtype = bid.mtype.unwrap_or(0);
                if let Some(bid_type) = get_media_type_for_imp(mtype) {
                    result.bids.push(TypedBid::new(bid, bid_type));
                }
                // else: skip bids with unknown mtype
            }
        }
        Ok(result)
    }
}
