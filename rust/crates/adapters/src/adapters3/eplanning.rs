use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;

pub struct EplanningAdapter {
    pub endpoint: String,
}

impl EplanningAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

const NULL_SIZE: &str = "1x1";
const DEFAULT_PAGE_URL: &str = "FILE";
const SEC: &str = "ROS";
const DFP_CLIENT_ID: &str = "1";
const REQUEST_TARGET_INVENTORY: &str = "1";
const VAST_INSTREAM: i32 = 1;
const VAST_OUTSTREAM: i32 = 2;
const VAST_VERSION_DEFAULT: &str = "3";
const VAST_DEFAULT_SIZE: &str = "640x480";
const IMP_TYPE_BANNER: i32 = 0;

static PRIORITY_MOBILE: &[&str] = &["1x1", "300x50", "320x50", "300x250"];
static PRIORITY_DESKTOP: &[&str] = &["1x1", "970x90", "970x250", "160x600", "300x600", "728x90", "300x250"];

#[derive(Deserialize)]
struct HbResponse {
    #[serde(rename = "sp")]
    spaces: Vec<HbResponseSpace>,
}

#[derive(Deserialize)]
struct HbResponseSpace {
    #[serde(rename = "k")]
    name: String,
    #[serde(rename = "a")]
    ads: Vec<HbResponseAd>,
}

#[derive(Deserialize)]
struct HbResponseAd {
    #[serde(rename = "i")]
    impression_id: String,
    #[serde(rename = "id", default)]
    ad_id: String,
    #[serde(rename = "pr")]
    price: String,
    #[serde(rename = "adm")]
    adm: String,
    #[serde(rename = "crid")]
    cr_id: String,
    #[serde(rename = "adom", default)]
    adomain: String,
    #[serde(rename = "w", default)]
    width: u64,
    #[serde(rename = "h", default)]
    height: u64,
}

#[derive(Debug, Default, Deserialize)]
struct ExtImpEPlanning {
    #[serde(rename = "clientID", default)]
    client_id: String,
    #[serde(rename = "adUnitCode", default)]
    ad_unit_code: String,
}

fn is_mobile_device(request: &openrtb::BidRequest) -> bool {
    request.device.as_ref().map(|d| {
        // device_type: 1=Mobile/Tablet, 4=Phone, 5=Tablet
        matches!(d.devicetype, Some(1) | Some(4) | Some(5))
    }).unwrap_or(false)
}

fn get_imp_type_request(request: &openrtb::BidRequest) -> i32 {
    let mut imp_type = IMP_TYPE_BANNER;
    for imp in &request.imp {
        if let Some(video) = &imp.video {
            let placement = video.placement.unwrap_or(0);
            if placement == VAST_INSTREAM {
                imp_type = VAST_INSTREAM;
            } else if imp_type == IMP_TYPE_BANNER {
                imp_type = VAST_OUTSTREAM;
            }
        }
    }
    imp_type
}

fn clean_name(name: &str) -> String {
    let mut s = name.to_string();
    // Replace _.-/ with ""
    s = s.replace(['_', '.', '-', '/'], "");
    // Replace )( ( ) : with _
    s = s.replace(")(", "_").replace('(', "_").replace(')', "_").replace(':', "_");
    // Trim leading/trailing _
    s = s.trim_matches('_').to_string();
    s
}

fn search_size_priority(hashed: &HashMap<String, usize>, formats: &[openrtb::Format], priority: &[&str]) -> (i32, i32) {
    for size_str in priority.iter().rev() {
        if let Some(&idx) = hashed.get(*size_str) {
            let f = &formats[idx];
            return (f.w.unwrap_or(0), f.h.unwrap_or(0));
        }
    }
    (formats[0].w.unwrap_or(0), formats[0].h.unwrap_or(0))
}

fn get_size_from_imp(imp: &openrtb::Imp, is_mobile: bool) -> (i32, i32) {
    if let Some(video) = &imp.video {
        if let (Some(w), Some(h)) = (video.w, video.h) {
            if w > 0 && h > 0 {
                return (w, h);
            }
        }
    }

    if let Some(banner) = &imp.banner {
        if let (Some(w), Some(h)) = (banner.w, banner.h) {
            return (w, h);
        }

        if let Some(formats) = &banner.format {
            if !formats.is_empty() {
                let mut hashed: HashMap<String, usize> = HashMap::new();
                for (i, f) in formats.iter().enumerate() {
                    if let (Some(w), Some(h)) = (f.w, f.h) {
                        if w != 0 && h != 0 {
                            hashed.insert(format!("{}x{}", w, h), i);
                        }
                    }
                }
                let priority = if is_mobile { PRIORITY_MOBILE } else { PRIORITY_DESKTOP };
                return search_size_priority(&hashed, formats, priority);
            }
        }
    }

    (0, 0)
}

fn get_name_video(size: &str, index: i32) -> String {
    format!("video_{}_{}", size, index)
}

fn verify_imp(imp: &openrtb::Imp, is_mobile: bool, imp_type: i32) -> Result<(String, String, String), BidderError> {
    // Validate video placement matching
    if imp_type > IMP_TYPE_BANNER {
        if imp_type == VAST_INSTREAM {
            match &imp.video {
                Some(v) if v.placement == Some(VAST_INSTREAM) => {}
                _ => return Err(BidderError::BadInput(format!(
                    "Ignoring imp id={}, auction instream and imp no instream", imp.id
                ))),
            }
        } else {
            // outstream
            match &imp.video {
                Some(v) if v.placement != Some(VAST_INSTREAM) => {}
                _ => return Err(BidderError::BadInput(format!(
                    "Ignoring imp id={}, auction outstream and imp no outstream", imp.id
                ))),
            }
        }
    }

    let bidder_ext = imp.ext.as_ref()
        .and_then(|e| e.get("bidder"))
        .ok_or_else(|| BidderError::BadInput(format!(
            "Ignoring imp id={}, error while decoding extImpBidder", imp.id
        )))?;

    let imp_ext: ExtImpEPlanning = serde_json::from_value(bidder_ext.clone())
        .map_err(|e| BidderError::BadInput(format!(
            "Ignoring imp id={}, error while decoding impExt, err: {}", imp.id, e
        )))?;

    if imp_ext.client_id.is_empty() {
        return Err(BidderError::BadInput(format!(
            "Ignoring imp id={}, no ClientID present", imp.id
        )));
    }

    let (w, h) = get_size_from_imp(imp, is_mobile);
    let size_str = if w == 0 && h == 0 {
        if imp.video.is_some() {
            VAST_DEFAULT_SIZE.to_string()
        } else {
            NULL_SIZE.to_string()
        }
    } else {
        format!("{}x{}", w, h)
    };

    let ad_unit_code = if imp_ext.ad_unit_code.is_empty() {
        size_str.clone()
    } else {
        imp_ext.ad_unit_code.clone()
    };

    Ok((imp_ext.client_id, ad_unit_code, size_str))
}

fn percent_encode(s: &str) -> String {
    let mut result = String::new();
    for c in s.chars() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => result.push(c),
            ' ' => result.push_str("%20"),
            c => {
                for byte in c.to_string().as_bytes() {
                    result.push_str(&format!("%{:02X}", byte));
                }
            }
        }
    }
    result
}

impl Bidder for EplanningAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut client_id = String::new();
        let mut spaces: Vec<String> = Vec::new();
        let mut vast_index = 0i32;

        let is_mobile = is_mobile_device(request);
        let imp_type = get_imp_type_request(request);

        for imp in &request.imp {
            match verify_imp(imp, is_mobile, imp_type) {
                Ok((c_id, ad_unit, size_str)) => {
                    if client_id.is_empty() {
                        client_id = c_id;
                    }

                    let name = clean_name(&ad_unit);
                    if imp.video.is_some() {
                        let vname = get_name_video(&size_str, vast_index);
                        spaces.push(format!("{}:{};1", vname, size_str));
                        vast_index += 1;
                    } else {
                        spaces.push(format!("{}:{}", name, size_str));
                    }
                }
                Err(e) => {
                    errs.push(e);
                    continue;
                }
            }
        }

        if client_id.is_empty() {
            return (vec![], errs);
        }

        let page_url = request.site.as_ref()
            .and_then(|s| s.page.as_deref())
            .unwrap_or(DEFAULT_PAGE_URL)
            .to_string();

        let page_domain = request.site.as_ref()
            .map(|s| {
                if let Some(d) = &s.domain {
                    if !d.is_empty() { return d.clone(); }
                }
                if let Some(p) = &s.page {
                    // Extract hostname from page URL
                    if let Some(start) = p.find("://") {
                        let rest = &p[start + 3..];
                        let end = rest.find('/').unwrap_or(rest.len());
                        return rest[..end].to_string();
                    }
                    return p.clone();
                }
                DEFAULT_PAGE_URL.to_string()
            })
            .unwrap_or_else(|| DEFAULT_PAGE_URL.to_string());

        let request_target = request.app.as_ref()
            .and_then(|a| a.bundle.as_deref())
            .unwrap_or(&page_domain)
            .to_string();

        let spaces_str = spaces.join("+");

        let mut query_parts: Vec<(String, String)> = vec![
            ("ncb".to_string(), "1".to_string()),
        ];

        if request.app.is_none() {
            query_parts.push(("ur".to_string(), page_url));
        }

        query_parts.push(("e".to_string(), spaces_str));

        if let Some(user) = &request.user {
            if let Some(buyer_uid) = &user.buyeruid {
                if !buyer_uid.is_empty() {
                    query_parts.push(("uid".to_string(), buyer_uid.clone()));
                }
            }
        }

        if let Some(device) = &request.device {
            if let Some(ip) = &device.ip {
                if !ip.is_empty() {
                    query_parts.push(("ip".to_string(), ip.clone()));
                }
            }
        }

        if let Some(app) = &request.app {
            if let Some(name) = &app.name {
                if !name.is_empty() {
                    query_parts.push(("appn".to_string(), name.clone()));
                }
            }
            if let Some(app_id) = &app.id {
                if !app_id.is_empty() {
                    query_parts.push(("appid".to_string(), app_id.clone()));
                }
            }
            if let Some(device) = &request.device {
                if let Some(ifa) = &device.ifa {
                    if !ifa.is_empty() {
                        query_parts.push(("ifa".to_string(), ifa.clone()));
                    }
                }
            }
            query_parts.push(("app".to_string(), REQUEST_TARGET_INVENTORY.to_string()));
        }

        if imp_type > 0 {
            query_parts.push(("vctx".to_string(), imp_type.to_string()));
            query_parts.push(("vv".to_string(), VAST_VERSION_DEFAULT.to_string()));
        }

        let query_string: String = query_parts.iter()
            .map(|(k, v)| format!("{}={}", k, percent_encode(v)))
            .collect::<Vec<_>>()
            .join("&");

        let uri = format!(
            "{}/{}/{}/{}/{}?{}",
            self.endpoint.trim_end_matches('/'),
            client_id,
            DFP_CLIENT_ID,
            request_target,
            SEC,
            query_string
        );

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        if let Some(device) = &request.device {
            if let Some(ua) = &device.ua {
                if !ua.is_empty() {
                    headers.insert("User-Agent".to_string(), ua.clone());
                }
            }
            if let Some(ip) = &device.ip {
                if !ip.is_empty() {
                    headers.insert("X-Forwarded-For".to_string(), ip.clone());
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

        (
            vec![RequestData {
                method: "GET".to_string(),
                uri,
                body: vec![],
                headers,
                imp_ids: get_imp_ids(&request.imp),
            }],
            errs,
        )
    }

    fn make_bids(
        &self,
        internal: &openrtb::BidRequest,
        _external: &RequestData,
        response: &ResponseData,
    ) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 {
            return Ok(BidderResponse::new());
        }
        if response.status_code == 400 {
            return Err(vec![BidderError::BadInput(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info",
                response.status_code
            ))]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info",
                response.status_code
            ))]);
        }

        let parsed: HbResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(format!(
                "Error unmarshaling HB response: {}", e
            ))])?;

        let is_mobile = is_mobile_device(internal);
        let imp_type = get_imp_type_request(internal);

        // Build space name -> imp id map
        let mut vast_index = 0i32;
        let mut space_to_imp: HashMap<String, String> = HashMap::new();
        for imp in &internal.imp {
            if let Ok((_, ad_unit, size_str)) = verify_imp(imp, is_mobile, imp_type) {
                let name = clean_name(&ad_unit);
                if imp.video.is_some() {
                    let vname = get_name_video(&size_str, vast_index);
                    space_to_imp.insert(vname, imp.id.clone());
                    vast_index += 1;
                } else {
                    space_to_imp.insert(name, imp.id.clone());
                }
            }
        }

        let bid_type = if imp_type > 0 { BidType::Video } else { BidType::Banner };
        let mut result = BidderResponse::new();

        for space in parsed.spaces {
            let imp_id = space_to_imp.get(&space.name).cloned().unwrap_or_default();
            for ad in space.ads {
                if let Ok(price) = ad.price.parse::<f64>() {
                    let bid = openrtb::Bid {
                        id: ad.impression_id,
                        impid: imp_id.clone(),
                        price,
                        adm: Some(ad.adm),
                        crid: Some(ad.cr_id),
                        w: Some(ad.width as i32),
                        h: Some(ad.height as i32),
                        adid: if ad.ad_id.is_empty() { None } else { Some(ad.ad_id) },
                        adomain: if ad.adomain.is_empty() { None } else { Some(vec![ad.adomain]) },
                        ..Default::default()
                    };
                    result.bids.push(TypedBid::new(bid, bid_type.clone()));
                }
            }
        }

        Ok(result)
    }
}
