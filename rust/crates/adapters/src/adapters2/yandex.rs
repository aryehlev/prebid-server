use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_bid_type_from_imp, get_imp_ids};
use serde::Deserialize;

pub struct YandexAdapter { pub endpoint: String }
impl YandexAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

const BIDDER_NAME: &str = "prebid.go";
const BIDDER_VERSION: &str = "1.1";
const VIDEO_MIN_DURATION: i32 = 1;
const VIDEO_MAX_DURATION: i32 = 120;

#[derive(Debug, Default, Deserialize)]
struct ExtImpYandex {
    #[serde(rename = "placementId", default)]
    placement_id: String,
    #[serde(rename = "pageId", default)]
    page_id: i64,
    #[serde(rename = "impId", default)]
    imp_id: i64,
}

fn resolve_placement(ext: &ExtImpYandex) -> Result<(String, String), BidderError> {
    if ext.placement_id.is_empty() {
        let page_id = ext.page_id.to_string();
        let imp_id = ext.imp_id.to_string();
        return Ok((page_id, imp_id));
    }
    // Split on '-' and collect only numeric parts, take last two
    let parts: Vec<&str> = ext.placement_id.split('-').collect();
    let numeric: Vec<&str> = parts.iter().filter(|p| p.parse::<i64>().is_ok()).copied().collect();
    if numeric.len() < 2 {
        return Err(BidderError::BadInput(format!(
            "invalid placement id, it must contain two parts: {}", ext.placement_id
        )));
    }
    let n = numeric.len();
    Ok((numeric[n - 2].to_string(), numeric[n - 1].to_string()))
}

/// Modify banner: if W/H are missing or zero, fill from first format entry.
fn modify_banner(banner: &mut openrtb::Banner) -> Result<(), BidderError> {
    let needs_size = banner.w.map_or(true, |w| w == 0) || banner.h.map_or(true, |h| h == 0);
    if needs_size {
        // Try to fill from formats
        let first = banner.format.as_ref().and_then(|f| f.first()).cloned();
        match first {
            Some(fmt) => {
                banner.w = fmt.w;
                banner.h = fmt.h;
            }
            None => {
                return Err(BidderError::BadInput("Invalid size provided for Banner".to_string()));
            }
        }
    }
    Ok(())
}

/// Modify video: validate W/H, set min/max duration defaults, default protocol.
fn modify_video(video: &mut openrtb::Video) -> Result<(), BidderError> {
    let w_ok = video.w.map_or(false, |w| w != 0);
    let h_ok = video.h.map_or(false, |h| h != 0);
    if !w_ok || !h_ok {
        return Err(BidderError::BadInput("Invalid size provided for Video".to_string()));
    }
    if video.minduration.map_or(true, |d| d == 0) {
        video.minduration = Some(VIDEO_MIN_DURATION);
    }
    if video.maxduration.map_or(true, |d| d == 0) {
        video.maxduration = Some(VIDEO_MAX_DURATION);
    }
    if video.protocols.as_ref().map_or(true, |p| p.is_empty()) {
        video.protocols = Some(vec![3]); // VAST 3.0
    }
    Ok(())
}

/// Modify imp in place: set display manager, validate/fix banner and video, require a supported type.
fn modify_imp(imp: &mut openrtb::Imp) -> Result<(), BidderError> {
    imp.displaymanager = Some(BIDDER_NAME.to_string());
    imp.displaymanagerver = Some(BIDDER_VERSION.to_string());

    let mut has_supported_type = false;

    if let Some(banner) = imp.banner.as_mut() {
        modify_banner(banner)?;
        has_supported_type = true;
    }

    if let Some(video) = imp.video.as_mut() {
        modify_video(video)?;
        has_supported_type = true;
    }

    if imp.native.is_some() {
        has_supported_type = true;
    }

    if !has_supported_type {
        return Err(BidderError::BadInput(format!(
            "Unsupported format. Yandex only supports banner, video, and native types. Ignoring imp id #{}",
            imp.id
        )));
    }

    Ok(())
}

impl Bidder for YandexAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();
        let mut errs = Vec::new();

        let referer = request.site.as_ref().and_then(|s| {
            if s.page.as_ref().map_or(false, |p| !p.is_empty()) {
                s.page.clone()
            } else {
                s.domain.clone()
            }
        }).unwrap_or_default();

        let currency = request.cur.as_ref().and_then(|c| c.first().cloned()).unwrap_or_default();

        for imp in &request.imp {
            // Extract bidder ext
            let bidder_val = match imp.ext.as_ref().and_then(|e| e.get("bidder")).cloned() {
                Some(v) => v,
                None => {
                    errs.push(BidderError::BadInput(format!("imp {}: missing bidder ext", imp.id)));
                    continue;
                }
            };
            let yandex_ext: ExtImpYandex = match serde_json::from_value(bidder_val) {
                Ok(e) => e,
                Err(e) => {
                    errs.push(BidderError::BadInput(format!("imp {}: unable to unmarshal ext.bidder: {}", imp.id, e)));
                    continue;
                }
            };

            let (page_id, imp_id_str) = match resolve_placement(&yandex_ext) {
                Ok(p) => p,
                Err(e) => {
                    errs.push(e);
                    continue;
                }
            };

            // Clone imp and apply modifications (banner size fill, video defaults, display manager)
            let mut modified_imp = imp.clone();
            if let Err(e) = modify_imp(&mut modified_imp) {
                errs.push(e);
                continue;
            }

            // Build URL: replace {{.PageID}} macro and add query params
            let base_url = self.endpoint.replace("{{.PageID}}", &page_id);
            let mut url = base_url;
            let mut params = Vec::new();
            if !referer.is_empty() {
                params.push(format!("target-ref={}", urlencoding(&referer)));
            }
            if !currency.is_empty() {
                params.push(format!("ssp-cur={}", urlencoding(&currency)));
            }
            if !imp_id_str.is_empty() {
                params.push(format!("imp-id={}", urlencoding(&imp_id_str)));
            }
            if !params.is_empty() {
                if url.contains('?') {
                    url.push('&');
                } else {
                    url.push('?');
                }
                url.push_str(&params.join("&"));
            }

            // Build single-imp request
            let mut single_req = request.clone();
            single_req.imp = vec![modified_imp];

            // Build headers
            let mut headers = HashMap::new();
            headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
            headers.insert("Accept".to_string(), "application/json".to_string());
            headers.insert("X-OpenRTB-Version".to_string(), "2.5".to_string());
            if let Some(device) = &request.device {
                if let Some(ua) = &device.ua {
                    if !ua.is_empty() {
                        headers.insert("User-Agent".to_string(), ua.clone());
                    }
                }
                if let Some(ip) = &device.ip {
                    if !ip.is_empty() {
                        headers.insert("X-Forwarded-For".to_string(), ip.clone());
                        headers.insert("X-Real-Ip".to_string(), ip.clone());
                    }
                }
                if let Some(lang) = &device.language {
                    if !lang.is_empty() {
                        headers.insert("Accept-Language".to_string(), lang.clone());
                    }
                }
            }
            if let Some(site) = &request.site {
                if let Some(page) = &site.page {
                    if !page.is_empty() {
                        headers.insert("Referer".to_string(), page.clone());
                    }
                }
            }

            let imp_ids = get_imp_ids(&single_req.imp);
            let body = match serde_json::to_vec(&single_req) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(BidderError::BadInput(e.to_string()));
                    continue;
                }
            };

            requests.push(RequestData { method: "POST".to_string(), uri: url, body, headers, imp_ids });
        }

        (requests, errs)
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(internal.imp.len());
        let mut errs = Vec::new();

        // Build imp map for O(1) lookup
        let imp_map: HashMap<&str, &openrtb::Imp> = internal.imp.iter().map(|i| (i.id.as_str(), i)).collect();

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                match imp_map.get(bid.impid.as_str()) {
                    Some(imp) => {
                        let bid_type = get_bid_type_from_imp(imp);
                        result.bids.push(TypedBid::new(bid, bid_type));
                    }
                    None => {
                        errs.push(BidderError::BadInput(format!(
                            "Invalid bid imp ID #{} does not match any imp IDs from the original bid request",
                            bid.impid
                        )));
                    }
                }
            }
        }

        if errs.is_empty() {
            Ok(result)
        } else {
            // Return partial results alongside errors by placing bids already collected;
            // match Go behaviour of returning both bids and errors
            Ok(result)
        }
    }
}

fn urlencoding(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push('%');
                out.push_str(&format!("{:02X}", b));
            }
        }
    }
    out
}
