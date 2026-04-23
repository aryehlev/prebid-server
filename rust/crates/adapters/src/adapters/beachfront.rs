use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use openrtb_ext::BidType;
use serde::{Deserialize, Serialize};

const NURL_VIDEO_ENDPOINT_SUFFIX: &str = "&prebidserver";
const DEFAULT_VIDEO_WIDTH: i64 = 300;
const DEFAULT_VIDEO_HEIGHT: i64 = 250;

pub struct BeachfrontAdapter {
    pub banner_endpoint: String,
    pub video_endpoint: String,
}

impl BeachfrontAdapter {
    pub fn new(banner_endpoint: String, video_endpoint: String) -> Self {
        Self { banner_endpoint, video_endpoint }
    }

    /// Create a single-endpoint adapter (banner and video share same base endpoint).
    pub fn new_single(endpoint: String) -> Self {
        let video_endpoint = format!("https://reachms.bfmio.com/bid.json?exchange_id");
        Self { banner_endpoint: endpoint, video_endpoint }
    }
}

// ----- Bidder extension structs -----

#[derive(Debug, Default, Deserialize)]
struct ExtImpBeachfront {
    #[serde(rename = "appId", default)]
    app_id: String,
    #[serde(rename = "appIds", default)]
    app_ids: AppIds,
    #[serde(rename = "bidfloor", default)]
    bid_floor: f64,
    #[serde(rename = "videoResponseType", default)]
    video_response_type: String,
}

#[derive(Debug, Default, Deserialize)]
struct AppIds {
    #[serde(default)]
    banner: String,
    #[serde(default)]
    video: String,
}

#[derive(Debug, Default, Deserialize)]
struct ImpExt {
    #[serde(default)]
    bidder: ExtImpBeachfront,
}

// ----- Banner request structs (custom Beachfront format) -----

#[derive(Debug, Serialize)]
struct BannerRequest {
    slots: Vec<BannerSlot>,
    #[serde(skip_serializing_if = "String::is_empty")]
    domain: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    page: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    referrer: String,
    secure: i8,
    #[serde(rename = "deviceOs", skip_serializing_if = "String::is_empty")]
    device_os: String,
    #[serde(rename = "deviceModel", skip_serializing_if = "String::is_empty")]
    device_model: String,
    #[serde(rename = "isMobile")]
    is_mobile: i8,
    #[serde(skip_serializing_if = "String::is_empty")]
    ua: String,
    dnt: i8,
    #[serde(rename = "adapterName")]
    adapter_name: String,
    #[serde(rename = "adapterVersion")]
    adapter_version: String,
    #[serde(rename = "requestId", skip_serializing_if = "String::is_empty")]
    request_id: String,
    real204: bool,
    #[serde(rename = "ip", skip_serializing_if = "String::is_empty")]
    ip: String,
}

#[derive(Debug, Serialize)]
struct BannerSlot {
    slot: String,
    id: String,
    bidfloor: f64,
    sizes: Vec<BannerSize>,
}

#[derive(Debug, Serialize)]
struct BannerSize {
    w: u64,
    h: u64,
}

// ----- Banner response -----

#[derive(Debug, Deserialize)]
struct BannerResponseSlot {
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

fn get_app_id(ext: &ExtImpBeachfront, media_type: &str) -> Result<String, BidderError> {
    if !ext.app_id.is_empty() {
        return Ok(ext.app_id.clone());
    }
    match media_type {
        "video" if !ext.app_ids.video.is_empty() => Ok(ext.app_ids.video.clone()),
        "banner" if !ext.app_ids.banner.is_empty() => Ok(ext.app_ids.banner.clone()),
        _ => Err(BidderError::BadInput(
            "unable to determine the appId(s) from the supplied extension".to_string(),
        )),
    }
}

fn is_secure(page: &str) -> i8 {
    if page.starts_with("https://") { 1 } else { 0 }
}

fn get_domain(page: &str) -> String {
    let without_proto = if let Some(rest) = page.find("//").map(|i| &page[i + 2..]) {
        rest
    } else {
        page
    };
    without_proto.split('/').next().unwrap_or("").to_string()
}

impl Bidder for BeachfrontAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut video_imps = Vec::new();
        let mut banner_imps = Vec::new();

        for imp in &request.imp {
            // Banner: must have at least one format with non-zero W and H
            if let Some(banner) = &imp.banner {
                if let Some(formats) = &banner.format {
                    if !formats.is_empty() {
                        if let Some(f) = formats.first() {
                            if f.w.unwrap_or(0) != 0 && f.h.unwrap_or(0) != 0 {
                                banner_imps.push(imp.clone());
                            }
                        }
                    }
                }
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
        let mut headers: HashMap<String, String> = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
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

        if let Some(user) = &request.user {
            if let Some(buyeruid) = &user.buyeruid {
                if !buyeruid.is_empty() {
                    headers.insert("Cookie".to_string(), format!("__io_cid={}", buyeruid));
                }
            }
        }

        // Build banner request
        if !banner_imps.is_empty() {
            let mut slots = Vec::new();
            let mut slot_imp_ids = Vec::new();

            for imp in &banner_imps {
                let imp_ext: ImpExt = match imp.ext.as_ref()
                    .and_then(|e| serde_json::from_value(e.clone()).ok())
                {
                    Some(e) => e,
                    None => {
                        errs.push(BidderError::BadInput(format!(
                            "ignoring imp id={}, error while decoding extImpBidder",
                            imp.id
                        )));
                        continue;
                    }
                };

                let app_id = match get_app_id(&imp_ext.bidder, "banner") {
                    Ok(id) => id,
                    Err(e) => { errs.push(e); continue; }
                };

                let bid_floor = if imp.bidfloor.unwrap_or(0.0) > 0.0 {
                    imp.bidfloor.unwrap_or(0.0)
                } else {
                    imp_ext.bidder.bid_floor
                };

                let sizes: Vec<BannerSize> = imp.banner.as_ref()
                    .and_then(|b| b.format.as_ref())
                    .map(|fmts| fmts.iter().map(|f| BannerSize {
                        w: f.w.unwrap_or(0) as u64,
                        h: f.h.unwrap_or(0) as u64,
                    }).collect())
                    .unwrap_or_default();

                slot_imp_ids.push(imp.id.clone());
                slots.push(BannerSlot {
                    slot: imp.id.clone(),
                    id: app_id,
                    bidfloor: bid_floor,
                    sizes,
                });
            }

            if !slots.is_empty() {
                // Determine page/domain/mobile from site or app
                let (page, domain, is_mobile) = if let Some(site) = &request.site {
                    let page = site.page.clone().unwrap_or_default();
                    let domain = if site.domain.as_deref().unwrap_or("").is_empty() {
                        get_domain(&page)
                    } else {
                        site.domain.clone().unwrap_or_default()
                    };
                    (page, domain, 0i8)
                } else if let Some(app) = &request.app {
                    let page = app.bundle.clone().unwrap_or_default();
                    let domain = if app.domain.as_deref().unwrap_or("").is_empty() {
                        get_domain(&app.domain.clone().unwrap_or_default())
                    } else {
                        app.domain.clone().unwrap_or_default()
                    };
                    (page, domain, 1i8)
                } else {
                    (String::new(), String::new(), 0i8)
                };

                let secure = is_secure(&page);
                let (ua, device_model, device_os, dnt, ip) = if let Some(device) = &request.device {
                    (
                        device.ua.clone().unwrap_or_default(),
                        device.model.clone().unwrap_or_default(),
                        device.os.clone().unwrap_or_default(),
                        device.dnt.unwrap_or(0) as i8,
                        device.ip.clone().unwrap_or_default(),
                    )
                } else {
                    (String::new(), String::new(), String::new(), 0i8, String::new())
                };

                let banner_req = BannerRequest {
                    slots,
                    domain,
                    page,
                    referrer: String::new(),
                    secure,
                    device_os,
                    device_model,
                    is_mobile,
                    ua,
                    dnt,
                    adapter_name: "BF_PREBID_S2S".to_string(),
                    adapter_version: "1.0.0".to_string(),
                    request_id: request.id.clone(),
                    real204: true,
                    ip,
                };

                match serde_json::to_vec(&banner_req) {
                    Ok(body) => {
                        requests.push(RequestData {
                            method: "POST".to_string(),
                            uri: self.banner_endpoint.clone(),
                            body,
                            headers: headers.clone(),
                            imp_ids: slot_imp_ids,
                        });
                    }
                    Err(e) => errs.push(BidderError::BadInput(e.to_string())),
                }
            }
        }

        // Build video requests (one per imp)
        for imp in &video_imps {
            let imp_ext: ImpExt = match imp.ext.as_ref()
                .and_then(|e| serde_json::from_value(e.clone()).ok())
            {
                Some(e) => e,
                None => {
                    errs.push(BidderError::BadInput(format!(
                        "ignoring imp id={}, error while decoding extImpBidder",
                        imp.id
                    )));
                    continue;
                }
            };

            let app_id = match get_app_id(&imp_ext.bidder, "video") {
                Ok(id) => id,
                Err(e) => { errs.push(e); continue; }
            };

            let is_nurl = imp_ext.bidder.video_response_type == "nurl";

            let mut req_copy = request.clone();
            // Clear banner from this video imp, clear ext
            let mut video_imp = imp.clone();
            video_imp.banner = None;
            video_imp.ext = None;

            // Default video dimensions if unset
            if let Some(video) = &mut video_imp.video {
                if video.w.unwrap_or(0) == 0 {
                    video.w = Some(DEFAULT_VIDEO_WIDTH as i32);
                }
                if video.h.unwrap_or(0) == 0 {
                    video.h = Some(DEFAULT_VIDEO_HEIGHT as i32);
                }
            }

            req_copy.imp = vec![video_imp];
            req_copy.ext = None;

            if req_copy.cur.as_deref().unwrap_or(&[]).is_empty() {
                req_copy.cur = Some(vec!["USD".to_string()]);
            }

            // For adm type, inject fake IP if device.ip is empty
            if !is_nurl {
                if let Some(device) = &mut req_copy.device {
                    if device.ip.as_deref().unwrap_or("").is_empty() {
                        device.ip = Some("255.255.255.255".to_string());
                    }
                } else {
                    req_copy.device = Some(openrtb::Device {
                        ip: Some("255.255.255.255".to_string()),
                        ..Default::default()
                    });
                }
            }

            let uri = if is_nurl {
                format!("{}={}{}", self.video_endpoint, app_id, NURL_VIDEO_ENDPOINT_SUFFIX)
            } else {
                format!("{}={}", self.video_endpoint, app_id)
            };

            let imp_ids = vec![req_copy.imp[0].id.clone()];

            let body_result = if is_nurl {
                // Prepend {"isPrebid":true, to the JSON
                serde_json::to_vec(&req_copy).map(|b| {
                    let prefix = br#"{"isPrebid":true,"#;
                    // Replace opening `{` with `{"isPrebid":true,`
                    if b.first() == Some(&b'{') {
                        let mut result = prefix.to_vec();
                        result.extend_from_slice(&b[1..]);
                        result
                    } else {
                        b
                    }
                })
            } else {
                serde_json::to_vec(&req_copy)
            };

            match body_result {
                Ok(body) => {
                    requests.push(RequestData {
                        method: "POST".to_string(),
                        uri,
                        body,
                        headers: headers.clone(),
                        imp_ids,
                    });
                }
                Err(e) => errs.push(BidderError::BadInput(e.to_string())),
            }
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

        let mut result = BidderResponse::with_capacity(5);

        // Determine if this was a banner or video request based on URI
        let is_video = external.uri.contains("reachms.bfmio.com") ||
            external.uri.contains("bid.json");

        if is_video {
            // Try standard OpenRTB response
            match serde_json::from_slice::<openrtb::BidResponse>(&response.body) {
                Ok(bid_resp) if !bid_resp.seatbid.is_empty() => {
                    let _is_nurl = external.uri.ends_with(NURL_VIDEO_ENDPOINT_SUFFIX);
                    for sb in bid_resp.seatbid {
                        for bid in sb.bid {
                            result.bids.push(TypedBid::new(bid, BidType::Video));
                        }
                    }
                    return Ok(result);
                }
                _ => {
                    return Err(vec![BidderError::BadServerResponse(
                        "server response failed to unmarshal as valid rtb. Run with request.debug = 1 for more info".to_string(),
                    )]);
                }
            }
        }

        // Banner: try custom banner response format first, then OpenRTB
        if let Ok(bid_resp) = serde_json::from_slice::<openrtb::BidResponse>(&response.body) {
            if !bid_resp.seatbid.is_empty() {
                for sb in bid_resp.seatbid {
                    for bid in sb.bid {
                        result.bids.push(TypedBid::new(bid, BidType::Banner));
                    }
                }
                return Ok(result);
            }
        }

        // Try custom Beachfront banner format
        match serde_json::from_slice::<Vec<BannerResponseSlot>>(&response.body) {
            Ok(banner_resp) => {
                for slot in banner_resp {
                    let bid = openrtb::Bid {
                        id: format!("{}Banner", slot.slot),
                        impid: slot.slot.clone(),
                        price: slot.price,
                        crid: Some(slot.crid),
                        adm: Some(slot.adm),
                        w: Some(slot.w as i32),
                        h: Some(slot.h as i32),
                        ..Default::default()
                    };
                    result.bids.push(TypedBid::new(bid, BidType::Banner));
                }
                Ok(result)
            }
            Err(_) => Err(vec![BidderError::BadServerResponse(
                "server response failed to unmarshal as valid rtb. Run with request.debug = 1 for more info".to_string(),
            )]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_banner_req() -> openrtb::BidRequest {
        openrtb::BidRequest {
            id: "r".to_string(),
            imp: vec![openrtb::Imp {
                id: "i1".to_string(),
                banner: Some(openrtb::Banner {
                    format: Some(vec![openrtb::Format { w: Some(300), h: Some(250), ..Default::default() }]),
                    ..Default::default()
                }),
                ext: Some(serde_json::json!({"bidder": {"appId": "app123"}})),
                ..Default::default()
            }],
            site: Some(openrtb::Site { page: Some("https://site.example.com/page".to_string()), ..Default::default() }),
            ..Default::default()
        }
    }

    #[test]
    fn test_make_requests_banner() {
        let adapter = BeachfrontAdapter::new(
            "https://display.bfmio.com/prebid_display".to_string(),
            "https://reachms.bfmio.com/bid.json?exchange_id".to_string(),
        );
        let (reqs, errs) = adapter.make_requests(&make_banner_req(), &ExtraRequestInfo::default());
        assert!(errs.is_empty(), "errs: {:?}", errs);
        assert_eq!(reqs.len(), 1);
        assert!(reqs[0].uri.contains("bfmio.com"));
        assert!(reqs[0].headers.contains_key("Content-Type"));
        assert!(!reqs[0].body.is_empty());
    }

    #[test]
    fn test_make_bids_banner() {
        let adapter = BeachfrontAdapter::new(
            "https://display.bfmio.com/prebid_display".to_string(),
            "https://reachms.bfmio.com/bid.json?exchange_id".to_string(),
        );
        let ext_req = RequestData {
            method: "POST".to_string(),
            uri: "https://display.bfmio.com/prebid_display".to_string(),
            body: vec![],
            headers: HashMap::new(),
            imp_ids: vec!["i1".to_string()],
        };
        let body = br#"[{"slot":"i1","crid":"c1","price":1.23,"w":300,"h":250,"adm":"<ad/>"}]"#;
        let resp = ResponseData::new(200, body.to_vec());
        let result = adapter.make_bids(&make_banner_req(), &ext_req, &resp).unwrap();
        assert_eq!(result.bids.len(), 1);
        assert_eq!(result.bids[0].bid_type, BidType::Banner);
    }
}
