use std::collections::HashMap;

use pbs_adapters::{
    Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid,
    get_imp_ids,
};
use openrtb_ext::{BidType, ExtBidPrebidVideo};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const DEFAULT_VIDEO_WIDTH: i32 = 300;
const DEFAULT_VIDEO_HEIGHT: i32 = 250;
const NURL_VIDEO_ENDPOINT_SUFFIX: &str = "&prebidserver";
const BEACHFRONT_ADAPTER_NAME: &str = "BF_PREBID_S2S";
const BEACHFRONT_ADAPTER_VERSION: &str = "1.0.0";
const MIN_BID_FLOOR: f64 = 0.01;

pub struct BeachfrontAdapter {
    pub banner_endpoint: String,
    pub video_endpoint: String,
}

impl BeachfrontAdapter {
    pub fn new(banner_endpoint: String, video_endpoint: String) -> Self {
        Self {
            banner_endpoint,
            video_endpoint,
        }
    }
}

/// Beachfront imp ext bidder params
#[derive(Debug, Default, Deserialize)]
struct ExtImpBeachfront {
    #[serde(rename = "appId", default)]
    app_id: String,
    #[serde(rename = "appIds", default)]
    app_ids: BeachfrontAppIds,
    #[serde(rename = "bidfloor", default)]
    bid_floor: f64,
    #[serde(rename = "videoResponseType", default)]
    video_response_type: String,
}

#[derive(Debug, Default, Deserialize)]
struct BeachfrontAppIds {
    #[serde(default)]
    banner: String,
    #[serde(default)]
    video: String,
}

#[derive(Debug, Deserialize)]
struct ImpExt {
    bidder: ExtImpBeachfront,
}

/// Beachfront banner request format
#[derive(Debug, Default, Serialize)]
struct BeachfrontBannerRequest {
    slots: Vec<BeachfrontSlot>,
    #[serde(skip_serializing_if = "str::is_empty")]
    domain: String,
    #[serde(skip_serializing_if = "str::is_empty")]
    page: String,
    secure: i8,
    #[serde(rename = "deviceOs", skip_serializing_if = "str::is_empty")]
    device_os: String,
    #[serde(rename = "deviceModel", skip_serializing_if = "str::is_empty")]
    device_model: String,
    #[serde(rename = "isMobile")]
    is_mobile: i8,
    #[serde(skip_serializing_if = "str::is_empty")]
    ua: String,
    dnt: i8,
    #[serde(rename = "adapterName")]
    adapter_name: String,
    #[serde(rename = "adapterVersion")]
    adapter_version: String,
    #[serde(skip_serializing_if = "str::is_empty")]
    ip: String,
    #[serde(rename = "requestId")]
    request_id: String,
    real204: bool,
}

#[derive(Debug, Serialize)]
struct BeachfrontSlot {
    slot: String,
    id: String,
    bidfloor: f64,
    sizes: Vec<BeachfrontSize>,
}

#[derive(Debug, Serialize)]
struct BeachfrontSize {
    w: u32,
    h: u32,
}

/// Beachfront banner response slot
#[derive(Debug, Default, Deserialize)]
struct BeachfrontResponseSlot {
    #[serde(rename = "crid", default)]
    crid: String,
    #[serde(default)]
    price: f64,
    #[serde(default)]
    w: u32,
    #[serde(default)]
    h: u32,
    #[serde(default)]
    slot: String,
    #[serde(default)]
    adm: String,
}

/// Beachfront video bid ext
#[derive(Debug, Default, Deserialize)]
struct BeachfrontVideoBidExt {
    #[serde(default)]
    duration: i32,
}

fn get_app_id(ext: &ExtImpBeachfront, media: BidType) -> Option<String> {
    if !ext.app_id.is_empty() {
        return Some(ext.app_id.clone());
    }
    match media {
        BidType::Video if !ext.app_ids.video.is_empty() => Some(ext.app_ids.video.clone()),
        BidType::Banner if !ext.app_ids.banner.is_empty() => Some(ext.app_ids.banner.clone()),
        _ => None,
    }
}

fn get_beachfront_ext(imp: &openrtb::Imp) -> Result<ExtImpBeachfront, BidderError> {
    let ext_val = imp.ext.as_ref()
        .ok_or_else(|| BidderError::BadInput("imp.ext is missing".to_string()))?;

    let imp_ext: ImpExt = serde_json::from_value(ext_val.clone())
        .map_err(|e| BidderError::BadInput(e.to_string()))?;

    Ok(imp_ext.bidder)
}

fn is_secure_page(page: &str) -> i8 {
    if page.starts_with("https") { 1 } else { 0 }
}

impl Bidder for BeachfrontAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut banner_imps = Vec::new();
        let mut video_imps = Vec::new();

        for imp in &request.imp {
            let has_valid_banner = imp.banner.as_ref().map_or(false, |b| {
                b.format.as_ref().map_or(false, |f| {
                    !f.is_empty() && f[0].h.unwrap_or(0) != 0 && f[0].w.unwrap_or(0) != 0
                })
            });
            if has_valid_banner {
                banner_imps.push(imp.clone());
            }
            if imp.video.is_some() {
                video_imps.push(imp.clone());
            }
        }

        if banner_imps.is_empty() && video_imps.is_empty() {
            errs.push(BidderError::BadInput(
                "no valid impressions were found in the request".to_string(),
            ));
            return (vec![], errs);
        }

        let mut requests = Vec::new();

        let mut headers = HashMap::new();
        headers.insert(
            "Content-Type".to_string(),
            "application/json;charset=utf-8".to_string(),
        );
        headers.insert("Accept".to_string(), "application/json".to_string());

        if let Some(device) = &request.device {
            if let Some(ua) = &device.ua {
                if !ua.is_empty() {
                    headers.insert("User-Agent".to_string(), ua.clone());
                }
            }
            if let Some(lang) = &device.language {
                if !lang.is_empty() {
                    headers.insert("Accept-Language".to_string(), lang.clone());
                }
            }
            if let Some(dnt) = device.dnt {
                headers.insert("DNT".to_string(), dnt.to_string());
            }
        }

        // Build banner request
        if !banner_imps.is_empty() {
            let mut bfr = BeachfrontBannerRequest {
                adapter_name: BEACHFRONT_ADAPTER_NAME.to_string(),
                adapter_version: BEACHFRONT_ADAPTER_VERSION.to_string(),
                real204: true,
                request_id: request.id.clone(),
                ..Default::default()
            };

            let mut valid_slots = true;
            for imp in &banner_imps {
                let bf_ext = match get_beachfront_ext(imp) {
                    Ok(e) => e,
                    Err(e) => { errs.push(e); continue; }
                };

                let app_id = match get_app_id(&bf_ext, BidType::Banner) {
                    Some(id) => id,
                    None => {
                        errs.push(BidderError::BadInput(
                            "unable to determine the appId(s) from the supplied extension".to_string(),
                        ));
                        continue;
                    }
                };

                let mut sizes = Vec::new();
                if let Some(banner) = &imp.banner {
                    if let Some(formats) = &banner.format {
                        for fmt in formats {
                            sizes.push(BeachfrontSize {
                                w: fmt.w.unwrap_or(0) as u32,
                                h: fmt.h.unwrap_or(0) as u32,
                            });
                        }
                    }
                }

                let bid_floor = if imp.bidfloor.unwrap_or(0.0) > MIN_BID_FLOOR {
                    imp.bidfloor.unwrap_or(0.0)
                } else {
                    bf_ext.bid_floor
                };

                bfr.slots.push(BeachfrontSlot {
                    id: app_id,
                    slot: imp.id.clone(),
                    bidfloor: bid_floor,
                    sizes,
                });
            }

            if let Some(device) = &request.device {
                bfr.ip = device.ip.clone().unwrap_or_default();
                bfr.device_model = device.model.clone().unwrap_or_default();
                bfr.device_os = device.os.clone().unwrap_or_default();
                bfr.dnt = device.dnt.unwrap_or(0) as i8;
                bfr.ua = device.ua.clone().unwrap_or_default();
            }

            // Determine page/domain from site or app
            if request.site.is_some() {
                let site = request.site.as_ref().unwrap();
                bfr.page = site.page.clone().unwrap_or_default();
                bfr.domain = if site.domain.as_ref().map_or(true, |d| d.is_empty()) {
                    extract_domain(&bfr.page)
                } else {
                    site.domain.clone().unwrap_or_default()
                };
                bfr.is_mobile = 0;
            } else if let Some(app) = &request.app {
                bfr.page = app.bundle.clone().unwrap_or_default();
                bfr.domain = app.domain.clone().unwrap_or_default();
                bfr.is_mobile = 1;
            }

            bfr.secure = is_secure_page(&bfr.page);

            if let Some(imp0) = banner_imps.first() {
                if let Some(s) = imp0.secure {
                    bfr.secure = s as i8;
                }
            }

            if !bfr.slots.is_empty() {
                if let Ok(body) = serde_json::to_vec(&bfr) {
                    let slot_ids: Vec<String> = bfr.slots.iter().map(|s| s.slot.clone()).collect();
                    requests.push(RequestData {
                        method: "POST".to_string(),
                        uri: self.banner_endpoint.clone(),
                        body,
                        headers: headers.clone(),
                        imp_ids: slot_ids,
                    });
                }
            }
        }

        // Add user cookie for video requests
        if let Some(user) = &request.user {
            if let Some(uid) = &user.buyeruid {
                if !uid.is_empty() {
                    headers.insert("Cookie".to_string(), format!("__io_cid={}", uid));
                }
            }
        }

        // Build video requests (one per imp)
        for imp in &video_imps {
            let bf_ext = match get_beachfront_ext(imp) {
                Ok(e) => e,
                Err(e) => { errs.push(e); continue; }
            };

            let app_id = match get_app_id(&bf_ext, BidType::Video) {
                Some(id) => id,
                None => {
                    errs.push(BidderError::BadInput(
                        "unable to determine the appId(s) from the supplied extension".to_string(),
                    ));
                    continue;
                }
            };

            let is_nurl = bf_ext.video_response_type == "nurl";

            let mut req_copy = request.clone();
            let mut imp_copy = imp.clone();
            imp_copy.banner = None;
            imp_copy.ext = None;
            let secure: i32 = 0;
            imp_copy.secure = Some(secure);

            // Default video dimensions if missing
            if let Some(video) = &imp_copy.video {
                let mut video_copy = video.clone();
                if video_copy.w.unwrap_or(0) == 0 {
                    video_copy.w = Some(DEFAULT_VIDEO_WIDTH);
                }
                if video_copy.h.unwrap_or(0) == 0 {
                    video_copy.h = Some(DEFAULT_VIDEO_HEIGHT);
                }
                imp_copy.video = Some(video_copy);
            }

            req_copy.imp = vec![imp_copy];
            req_copy.ext = None;

            if req_copy.cur.as_ref().map_or(true, |c| c.is_empty()) {
                req_copy.cur = Some(vec!["USD".to_string()]);
            }

            let uri = if is_nurl {
                format!("{}={}{}", self.video_endpoint, app_id, NURL_VIDEO_ENDPOINT_SUFFIX)
            } else {
                format!("{}={}", self.video_endpoint, app_id)
            };

            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => {
                    if is_nurl {
                        // Prepend {"isPrebid":true, to the JSON
                        let mut full = b"{"isPrebid\":true,".to_vec();
                        if b.len() > 1 {
                            full.extend_from_slice(&b[1..]);
                        }
                        full
                    } else {
                        b
                    }
                }
                Err(e) => {
                    errs.push(BidderError::BadInput(e.to_string()));
                    continue;
                }
            };

            let imp_ids = get_imp_ids(&req_copy.imp);
            requests.push(RequestData {
                method: "POST".to_string(),
                uri,
                body,
                headers: headers.clone(),
                imp_ids,
            });
        }

        (requests, errs)
    }

    fn make_bids(
        &self,
        _internal: &openrtb::BidRequest,
        external: &RequestData,
        response: &ResponseData,
    ) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 {
            return Ok(BidderResponse::new());
        }

        if response.status_code >= 500 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "server error status code {} from {}. Run with request.debug = 1 for more info",
                response.status_code, external.uri
            ))]);
        }

        if response.status_code >= 400 {
            return Err(vec![BidderError::BadInput(format!(
                "request error status code {} from {}. Run with request.debug = 1 for more info",
                response.status_code, external.uri
            ))]);
        }

        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "unexpected status code {} from {}. Run with request.debug = 1 for more info",
                response.status_code, external.uri
            ))]);
        }

        // Determine bid type from URI
        let video_base = external.uri.split('=').next().unwrap_or("");
        let is_video = video_base == self.video_endpoint;

        let bid_type = if is_video {
            BidType::Video
        } else {
            BidType::Banner
        };

        let mut result = BidderResponse::with_capacity(5);

        if is_video {
            // Try to parse as OpenRTB bid response
            if let Ok(bid_response) = serde_json::from_slice::<openrtb::BidResponse>(&response.body) {
                if !bid_response.seatbid.is_empty() {
                    // Parse the external request to get imp info
                    let ext_request: Option<openrtb::BidRequest> =
                        serde_json::from_slice(&external.body).ok();

                    for (i, seat_bid) in bid_response.seatbid.iter().enumerate() {
                        for (j, bid) in seat_bid.bid.iter().enumerate() {
                            let mut bid_copy = bid.clone();

                            // For video, set impid from external request imps
                            if let Some(req) = &ext_request {
                                let is_nurl = external.uri.ends_with(NURL_VIDEO_ENDPOINT_SUFFIX);
                                if is_nurl {
                                    // NURL video: fix up bid fields
                                    if let Some(imp) = req.imp.get(j) {
                                        bid_copy.impid = imp.id.clone();
                                        if let Some(video) = &imp.video {
                                            bid_copy.w = video.w;
                                            bid_copy.h = video.h;
                                        }
                                        bid_copy.id = format!("{}NurlVideo", imp.id);
                                    }
                                } else {
                                    bid_copy.id = format!("{}AdmVideo", bid_copy.impid);
                                }
                            }

                            let mut dur = BeachfrontVideoBidExt::default();
                            let bid_video = bid_copy.ext.as_ref()
                                .and_then(|e| serde_json::from_value::<BeachfrontVideoBidExt>(e.clone()).ok())
                                .filter(|d| d.duration > 0)
                                .map(|d| {
                                    let primary_category = bid_copy.cat.as_ref()
                                        .and_then(|c| c.first())
                                        .cloned()
                                        .unwrap_or_default();
                                    ExtBidPrebidVideo {
                                        duration: d.duration,
                                        primary_category,
                                    }
                                });

                            let mut typed_bid = TypedBid::new(bid_copy, bid_type);
                            typed_bid.bid_video = bid_video;
                            result.bids.push(typed_bid);
                        }
                    }
                    return Ok(result);
                }
            }
        }

        // Try banner response format
        if let Ok(banner_resp) = serde_json::from_slice::<Vec<BeachfrontResponseSlot>>(&response.body) {
            for slot in banner_resp {
                let bid = openrtb::Bid {
                    id: format!("{}Banner", slot.slot),
                    impid: slot.slot.clone(),
                    price: slot.price,
                    adm: Some(slot.adm),
                    crid: Some(slot.crid),
                    w: Some(slot.w as i32),
                    h: Some(slot.h as i32),
                    ..Default::default()
                };
                result.bids.push(TypedBid::new(bid, BidType::Banner));
            }
            return Ok(result);
        }

        Err(vec![BidderError::BadServerResponse(
            "server response failed to unmarshal as valid rtb. Run with request.debug = 1 for more info".to_string(),
        )])
    }
}

fn extract_domain(page: &str) -> String {
    if let Some(without_proto) = page.strip_prefix("https://").or_else(|| page.strip_prefix("http://")) {
        without_proto.split('/').next().unwrap_or("").to_string()
    } else {
        String::new()
    }
}
