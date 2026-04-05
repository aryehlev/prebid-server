use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use openrtb_ext::BidType;
use serde::Deserialize;
use serde_json::Value;

pub struct AdgenerationAdapter { pub endpoint: String }
impl AdgenerationAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

#[derive(Deserialize)]
struct ExtImpBidder { bidder: Value }

#[derive(Deserialize)]
struct ExtImpAdgeneration { id: String }

#[derive(Deserialize, Default)]
struct AdgServerResponse {
    #[serde(default)]
    locationid: String,
    #[serde(default)]
    dealid: String,
    #[serde(default)]
    ad: String,
    #[serde(default)]
    beacon: String,
    #[serde(default)]
    beaconurl: String,
    #[serde(default)]
    cpm: f64,
    #[serde(default)]
    creativeid: String,
    #[serde(default)]
    h: u64,
    #[serde(default)]
    w: u64,
    #[serde(default)]
    vastxml: String,
    #[serde(default)]
    landing_url: String,
    #[serde(default)]
    results: Vec<Value>,
}

fn insert_vast_method(bid_id: &str, vastxml: &str) -> String {
    // Remove newlines from vastxml
    let replaced = vastxml.replace('\n', "").replace('\r', "");
    format!(
        "<script type=\"text/javascript\"> (function(){{ new APV.VideoAd({{s:\"{bid_id}\"}}).load('{replaced}'); }})(); </script>"
    )
}

fn append_child_to_body(ad: &str, data: &str) -> String {
    // Replace </body> or </ body> with data + </body>
    let re = regex::Regex::new(r"</\s?body>").unwrap();
    re.replace_all(ad, format!("{data}</body>").as_str()).to_string()
}

fn remove_wrapper(ad: &str) -> String {
    let body_start = ad.find("<body>");
    let body_end = ad.rfind("</body>");
    match (body_start, body_end) {
        (Some(start), Some(end)) => {
            let inner = &ad[start..end];
            let stripped = inner
                .replacen("<body>", "", 1)
                .replace("</body>", "");
            stripped.trim().to_string()
        }
        _ => String::new(),
    }
}

fn create_ad(body: &AdgServerResponse, imp_id: &str) -> String {
    let mut ad = if !body.vastxml.is_empty() {
        format!(
            "<body><div id=\"apvad-{imp_id}\"></div>\
             <script type=\"text/javascript\" id=\"apv\" src=\"https://cdn.apvdr.com/js/VideoAd.min.js\"></script>\
             {}</body>",
            insert_vast_method(imp_id, &body.vastxml)
        )
    } else {
        body.ad.clone()
    };
    ad = append_child_to_body(&ad, &body.beacon);
    let unwrapped = remove_wrapper(&ad);
    if !unwrapped.is_empty() {
        unwrapped
    } else {
        ad
    }
}

fn get_sizes(imp: &openrtb::Imp) -> String {
    if let Some(banner) = &imp.banner {
        let parts: Vec<String> = banner.format.as_deref().unwrap_or(&[]).iter()
            .map(|f| format!("{}x{}", f.w.unwrap_or(0), f.h.unwrap_or(0)))
            .collect();
        parts.join(",")
    } else {
        String::new()
    }
}

fn get_currency(request: &openrtb::BidRequest) -> String {
    let cur = match request.cur.as_deref() {
        None | Some([]) => return "JPY".to_string(),
        Some(c) => c,
    };
    for c in cur { if c == "JPY" { return c.clone(); } }
    cur[0].clone()
}

fn unmarshal_adg_ext(imp: &openrtb::Imp) -> Option<ExtImpAdgeneration> {
    let bidder_ext: ExtImpBidder = serde_json::from_value(imp.ext.clone()?).ok()?;
    serde_json::from_value(bidder_ext.bidder).ok()
}

impl Bidder for AdgenerationAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::BadInput("No impression in the bid request".to_string())]);
        }
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
        if let Some(device) = &request.device {
            if let Some(ua) = &device.ua { if !ua.is_empty() { headers.insert("User-Agent".to_string(), ua.clone()); } }
            if let Some(ip) = &device.ip { if !ip.is_empty() { headers.insert("X-Forwarded-For".to_string(), ip.clone()); } }
        }
        let currency = get_currency(request);
        let mut requests = Vec::new();
        let mut errs = Vec::new();
        for imp in &request.imp {
            let ext = match unmarshal_adg_ext(imp) {
                Some(e) if !e.id.is_empty() => e,
                _ => { errs.push(BidderError::BadInput(format!("imp {} missing valid ext.bidder.id", imp.id))); continue; }
            };
            let sizes = get_sizes(imp);
            let sdk_type = match request.device.as_ref().and_then(|d| d.os.as_deref()) {
                Some("android") => "1", Some("ios") => "2", _ => "0",
            };
            let mut uri = format!("{}?posall=SSPLOC&id={}&hb=true&t=json3&currency={}&sdkname=prebidserver&adapterver=1.0.3&sdktype={}",
                self.endpoint, ext.id, currency, sdk_type);
            if !sizes.is_empty() { uri.push_str(&format!("&sizes={}", sizes)); }
            if let Some(site) = &request.site {
                if let Some(p) = &site.page { if !p.is_empty() { uri.push_str(&format!("&tp={}", p)); } }
            }
            if let Some(app) = &request.app {
                if let Some(b) = &app.bundle { if !b.is_empty() { uri.push_str(&format!("&appbundle={}", b)); } }
                if let Some(n) = &app.name { if !n.is_empty() { uri.push_str(&format!("&appname={}", n)); } }
            }
            if let Some(device) = &request.device {
                let os = device.os.as_deref().unwrap_or("");
                let ifa = device.ifa.as_deref().unwrap_or("");
                if !ifa.is_empty() {
                    if os == "ios" { uri.push_str(&format!("&idfa={}", ifa)); }
                    if os == "android" { uri.push_str(&format!("&advertising_id={}", ifa)); }
                }
            }
            requests.push(RequestData { method: "GET".to_string(), uri, body: vec![], headers: headers.clone(), imp_ids: vec![imp.id.clone()] });
        }
        (requests, errs)
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: AdgServerResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        if bid_resp.results.as_ref().map(|r| r.is_empty()).unwrap_or(true) { return Ok(BidderResponse::new()); }
        let location_id = bid_resp.locationid.as_deref().unwrap_or("");
        for imp in &internal.imp {
            let ext = match unmarshal_adg_ext(imp) { Some(e) => e, None => continue };
            if ext.id == location_id {
                let bid = openrtb::Bid {
                    id: location_id.to_string(),
                    impid: imp.id.clone(),
                    price: bid_resp.cpm.unwrap_or(0.0),
                    adm: bid_resp.ad.clone(),
                    crid: bid_resp.creativeid.clone(),
                    dealid: bid_resp.dealid.clone(),
                    w: bid_resp.w.map(|v| v as i32),
                    h: bid_resp.h.map(|v| v as i32),
                    ..Default::default()
                };
                let mut result = BidderResponse::with_capacity(1);
                result.currency = get_currency(internal);
                result.bids.push(TypedBid::new(bid, BidType::Banner));
                return Ok(result);
            }
        }
        Ok(BidderResponse::new())
    }
}
