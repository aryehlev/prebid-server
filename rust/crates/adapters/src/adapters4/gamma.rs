use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use openrtb_ext::BidType;
use serde::Deserialize;

pub struct GammaAdapter { pub endpoint: String }
impl GammaAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

#[derive(Debug, Default, Deserialize)]
struct ExtImpGamma {
    #[serde(rename = "id", default)]
    partner_id: String,
    #[serde(rename = "zid", default)]
    zone_id: String,
    #[serde(rename = "wid", default)]
    web_id: String,
}

#[derive(Debug, Default, Deserialize)]
struct GammaBid {
    #[serde(rename = "id", default)]
    id: String,
    #[serde(rename = "impid", default)]
    impid: String,
    #[serde(rename = "price", default)]
    price: f64,
    #[serde(rename = "adid", default)]
    adid: String,
    #[serde(rename = "adm", default)]
    adm: String,
    #[serde(rename = "adomain", default)]
    adomain: Vec<String>,
    #[serde(rename = "crid", default)]
    crid: String,
    #[serde(rename = "w", default)]
    w: i32,
    #[serde(rename = "h", default)]
    h: i32,
    #[serde(rename = "vastXml", default)]
    vast_xml: String,
    #[serde(rename = "vastUrl", default)]
    vast_url: String,
}

#[derive(Debug, Default, Deserialize)]
struct GammaSeatBid {
    #[serde(rename = "bid", default)]
    bid: Vec<GammaBid>,
}

#[derive(Debug, Default, Deserialize)]
struct GammaBidResponse {
    #[serde(rename = "id", default)]
    id: String,
    #[serde(rename = "seatbid", default)]
    seatbid: Vec<GammaSeatBid>,
    #[serde(rename = "cur", default)]
    cur: String,
}

fn get_media_type_for_imp(imp_id: &str, imps: &[openrtb::Imp]) -> BidType {
    for imp in imps {
        if imp.id == imp_id {
            if imp.video.is_some() {
                return BidType::Video;
            }
            return BidType::Banner;
        }
    }
    BidType::Banner
}

fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => out.push(c),
            _ => {
                let bytes = c.to_string();
                for b in bytes.as_bytes() {
                    out.push_str(&format!("%{:02X}", b));
                }
            }
        }
    }
    out
}

fn make_single_request(endpoint: &str, request: &openrtb::BidRequest, imp: &openrtb::Imp) -> Result<RequestData, BidderError> {
    // Extract bidder ext
    let bidder_val = imp.ext.as_ref()
        .and_then(|e| e.get("bidder"))
        .cloned()
        .ok_or_else(|| BidderError::BadInput("ext.bidder not provided".to_string()))?;

    let gamma_ext: ExtImpGamma = serde_json::from_value(bidder_val)
        .map_err(|_| BidderError::BadInput("ext.bidder.publisher not provided".to_string()))?;

    if gamma_ext.partner_id.is_empty() {
        return Err(BidderError::BadInput("PartnerID is empty".to_string()));
    }
    if gamma_ext.zone_id.is_empty() {
        return Err(BidderError::BadInput("ZoneID is empty".to_string()));
    }
    if gamma_ext.web_id.is_empty() {
        return Err(BidderError::BadInput("WebID is empty".to_string()));
    }

    let mut uri = format!(
        "{}?id={}&zid={}&wid={}&bidid={}&hb=pbmobile",
        endpoint,
        gamma_ext.partner_id,
        gamma_ext.zone_id,
        gamma_ext.web_id,
        imp.id,
    );

    if let Some(device) = &request.device {
        if !device.ip.as_deref().unwrap_or("").is_empty() {
            uri.push_str(&format!("&device_ip={}", device.ip.as_deref().unwrap_or("")));
        }
        if !device.model.as_deref().unwrap_or("").is_empty() {
            uri.push_str(&format!("&device_model={}", device.model.as_deref().unwrap_or("")));
        }
        if !device.os.as_deref().unwrap_or("").is_empty() {
            uri.push_str(&format!("&device_os={}", device.os.as_deref().unwrap_or("")));
        }
        if !device.ua.as_deref().unwrap_or("").is_empty() {
            uri.push_str(&format!("&device_ua={}", url_encode(device.ua.as_deref().unwrap_or(""))));
        }
        if !device.ifa.as_deref().unwrap_or("").is_empty() {
            uri.push_str(&format!("&device_ifa={}", device.ifa.as_deref().unwrap_or("")));
        }
    }

    if let Some(app) = &request.app {
        if !app.id.as_deref().unwrap_or("").is_empty() {
            uri.push_str(&format!("&app_id={}", app.id.as_deref().unwrap_or("")));
        }
        if !app.bundle.as_deref().unwrap_or("").is_empty() {
            uri.push_str(&format!("&app_bundle={}", app.bundle.as_deref().unwrap_or("")));
        }
        if !app.name.as_deref().unwrap_or("").is_empty() {
            uri.push_str(&format!("&app_name={}", app.name.as_deref().unwrap_or("")));
        }
    }

    let mut headers = HashMap::new();
    headers.insert("Accept".to_string(), "*/*".to_string());
    headers.insert("x-openrtb-version".to_string(), "2.5".to_string());
    headers.insert("Connection".to_string(), "keep-alive".to_string());
    headers.insert("cache-control".to_string(), "no-cache".to_string());
    headers.insert("Accept-Encoding".to_string(), "gzip, deflate".to_string());

    if let Some(device) = &request.device {
        if !device.ua.as_deref().unwrap_or("").is_empty() {
            headers.insert("User-Agent".to_string(), device.ua.as_deref().unwrap_or("").to_string());
        }
        if !device.ip.as_deref().unwrap_or("").is_empty() {
            headers.insert("X-Forwarded-For".to_string(), device.ip.as_deref().unwrap_or("").to_string());
        }
        if !device.language.as_deref().unwrap_or("").is_empty() {
            headers.insert("Accept-Language".to_string(), device.language.as_deref().unwrap_or("").to_string());
        }
        if let Some(dnt) = device.dnt {
            headers.insert("DNT".to_string(), dnt.to_string());
        }
    }

    Ok(RequestData {
        method: "GET".to_string(),
        uri,
        body: Vec::new(),
        headers,
        imp_ids: vec![imp.id.clone()],
    })
}

impl Bidder for GammaAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();

        if request.imp.is_empty() {
            return (vec![], vec![BidderError::BadInput("No impressions in the bid request".to_string())]);
        }

        // Classify imps: fix banner w/h, skip non-banner/video
        let mut invalid_indices = Vec::new();
        let mut req_copy = request.clone();

        for i in 0..req_copy.imp.len() {
            if req_copy.imp[i].banner.is_some() {
                let banner = req_copy.imp[i].banner.as_mut().unwrap();
                if banner.w.is_none() && banner.h.is_none() {
                    if let Some(fmt) = banner.format.as_deref().and_then(|f| f.first()).cloned() {
                        banner.w = fmt.w;
                        banner.h = fmt.h;
                    }
                }
            } else if req_copy.imp[i].video.is_none() {
                errs.push(BidderError::BadInput(format!(
                    "Gamma only supports banner and video media types. Ignoring imp id={}",
                    req_copy.imp[i].id
                )));
                invalid_indices.push(i);
            }
        }

        // If all invalid
        if invalid_indices.len() == req_copy.imp.len() {
            errs.push(BidderError::BadInput("No valid impression in the bid request".to_string()));
            return (vec![], errs);
        }

        let mut adapter_requests = Vec::new();

        if invalid_indices.is_empty() {
            for imp in &req_copy.imp {
                match make_single_request(&self.endpoint, &req_copy, imp) {
                    Ok(r) => adapter_requests.push(r),
                    Err(e) => errs.push(e),
                }
            }
        } else {
            let mut j = 0usize;
            for i in 0..req_copy.imp.len() {
                if j < invalid_indices.len() && i == invalid_indices[j] {
                    j += 1;
                } else {
                    match make_single_request(&self.endpoint, &req_copy, &req_copy.imp[i]) {
                        Ok(r) => adapter_requests.push(r),
                        Err(e) => errs.push(e),
                    }
                }
            }
        }

        (adapter_requests, errs)
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code == 400 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info", response.status_code
            ))]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info", response.status_code
            ))]);
        }

        let gamma_resp: GammaBidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(format!("bad server response: {}. ", e))])?;

        if gamma_resp.seatbid.is_empty() {
            return Ok(BidderResponse::new());
        }

        let cap = gamma_resp.seatbid.get(0).map(|sb| sb.bid.len()).unwrap_or(0);
        let mut result = BidderResponse::with_capacity(cap);
        let mut errs = Vec::new();

        for sb in &gamma_resp.seatbid {
            for gbid in &sb.bid {
                let media_type = get_media_type_for_imp(&gamma_resp.id, &internal.imp);

                // Convert GammaBid to openrtb::Bid
                let mut bid = openrtb::Bid {
                    id: gbid.id.clone(),
                    impid: gbid.impid.clone(),
                    price: gbid.price,
                    adid: if gbid.adid.is_empty() { None } else { Some(gbid.adid.clone()) },
                    adm: if gbid.adm.is_empty() { None } else { Some(gbid.adm.clone()) },
                    adomain: if gbid.adomain.is_empty() { None } else { Some(gbid.adomain.clone()) },
                    crid: if gbid.crid.is_empty() { None } else { Some(gbid.crid.clone()) },
                    w: if gbid.w == 0 { None } else { Some(gbid.w) },
                    h: if gbid.h == 0 { None } else { Some(gbid.h) },
                    nurl: None,
                    ..Default::default()
                };

                if media_type == BidType::Video {
                    if !gbid.vast_xml.is_empty() {
                        if !gbid.vast_url.is_empty() {
                            bid.nurl = Some(gbid.vast_url.clone());
                        }
                        bid.adm = Some(gbid.vast_xml.clone());
                    } else {
                        errs.push(BidderError::BadServerResponse(
                            "Missing Ad Markup. Run with request.debug = 1 for more info".to_string()
                        ));
                        continue;
                    }
                } else if bid.adm.as_deref().unwrap_or("").is_empty() {
                    errs.push(BidderError::BadServerResponse(
                        "Missing Ad Markup. Run with request.debug = 1 for more info".to_string()
                    ));
                    continue;
                }

                result.bids.push(TypedBid::new(bid, media_type));
            }
        }

        Ok(result)
    }
}
