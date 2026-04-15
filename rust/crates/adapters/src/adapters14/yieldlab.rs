use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::{Deserialize, Serialize};

pub struct YieldlabAdapter { pub endpoint: String }
impl YieldlabAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

// --- Yieldlab-specific imp extension types ---

#[derive(Debug, Deserialize, Default, Clone)]
struct ExtImpYieldlab {
    #[serde(rename = "adslotId", default)]
    adslot_id: String,
    #[serde(rename = "supplyId", default)]
    supply_id: String,
    #[serde(rename = "targeting", default)]
    targeting: HashMap<String, String>,
    #[serde(rename = "extId", default)]
    ext_id: String,
}

// --- Yieldlab native response types ---

#[derive(Debug, Deserialize, Clone)]
struct YieldlabBidResponse {
    pub id: u64,
    pub price: u64,
    #[serde(default)]
    pub advertiser: String,
    #[serde(default)]
    pub adsize: String,
    #[serde(default)]
    pub pid: u64,
    #[serde(default)]
    pub pvid: String,
    #[serde(default)]
    pub dsa: Option<DsaResponse>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct DsaResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub behalf: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adrender: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transparency: Option<Vec<DsaTransparency>>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct DsaTransparency {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(rename = "dsaparams", skip_serializing_if = "Option::is_none")]
    pub params: Option<Vec<i32>>,
}

#[derive(Debug, Serialize)]
struct ResponseExtWithDsa {
    pub dsa: DsaResponse,
}

// --- DSA request types from regs.ext ---

#[derive(Debug, Deserialize)]
struct DsaRequest {
    #[serde(rename = "dsarequired")]
    pub required: Option<i32>,
    #[serde(rename = "pubrender")]
    pub pub_render: Option<i32>,
    #[serde(rename = "datatopub")]
    pub data_to_pub: Option<i32>,
    #[serde(default)]
    pub transparency: Vec<DsaTransparencyRequest>,
}

#[derive(Debug, Deserialize)]
struct DsaTransparencyRequest {
    #[serde(default)]
    pub domain: String,
    #[serde(rename = "dsaparams", default)]
    pub params: Vec<i32>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize, Default)]
struct ExtRegsWithDsa {
    #[serde(default)]
    pub gdpr: Option<i32>,
    #[serde(default)]
    pub us_privacy: Option<String>,
    #[serde(default)]
    pub dsa: Option<DsaRequest>,
}

#[derive(Debug, Deserialize, Default)]
struct ExtUser {
    #[serde(default)]
    pub consent: String,
}

// --- Supply chain types ---

#[derive(Debug, Deserialize)]
struct ExtRequestPrebidSchain {
    #[serde(default)]
    pub schain: SupplyChain,
}

#[derive(Debug, Deserialize, Default)]
struct SupplyChain {
    #[serde(default)]
    pub ver: String,
    #[serde(default)]
    pub complete: i32,
    #[serde(default)]
    pub nodes: Vec<SupplyChainNode>,
}

#[derive(Debug, Deserialize, Default)]
struct SupplyChainNode {
    #[serde(default)]
    pub asi: String,
    #[serde(default)]
    pub sid: String,
    pub hp: Option<i8>,
    #[serde(default)]
    pub rid: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub domain: String,
    pub ext: Option<serde_json::Value>,
}

// --- Helper functions ---

fn cache_buster() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

fn get_week() -> String {
    // Simple ISO week calculation: approximate with day-of-year / 7
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // 1970-01-01 was a Thursday (day 4 of week). Use simple modulo approximation.
    // days since epoch
    let days = now / 86400;
    // ISO week: (days + 3) / 7, 1-indexed, approximate
    let week = ((days + 3) / 7) % 53 + 1;
    week.to_string()
}

fn url_encode(s: &str) -> String {
    let mut encoded = String::new();
    for c in s.chars() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => encoded.push(c),
            ' ' => encoded.push('+'),
            _ => {
                let bytes = c.to_string().into_bytes();
                for b in bytes {
                    encoded.push_str(&format!("%{:02X}", b));
                }
            }
        }
    }
    encoded
}

fn make_targeting_values(targeting: &HashMap<String, String>) -> String {
    let mut parts: Vec<String> = targeting
        .iter()
        .map(|(k, v)| format!("{}={}", url_encode(k), url_encode(v)))
        .collect();
    parts.sort(); // stable output
    parts.join("&")
}

fn split_size(adsize: &str) -> Option<(i32, i32)> {
    let parts: Vec<&str> = adsize.splitn(2, 'x').collect();
    if parts.len() != 2 {
        return None;
    }
    let w = parts[0].parse::<i32>().ok()?;
    let h = parts[1].parse::<i32>().ok()?;
    Some((w, h))
}

fn imp_is_banner_only(imp: &openrtb::Imp) -> bool {
    imp.banner.is_some()
        && imp.video.is_none()
        && imp.audio.is_none()
        && imp.native.is_none()
}

fn parse_ext_imp_yieldlab(imp: &openrtb::Imp) -> Option<ExtImpYieldlab> {
    let ext = imp.ext.as_ref()?;
    let bidder_val = ext.get("bidder")?;
    serde_json::from_value(bidder_val.clone()).ok()
}

fn make_formats(request: &openrtb::BidRequest, all_params: &[ExtImpYieldlab]) -> Option<String> {
    // Map adslot_id -> params index for lookup
    let mut adslot_map: HashMap<&str, usize> = HashMap::new();
    for (i, p) in all_params.iter().enumerate() {
        adslot_map.insert(p.adslot_id.as_str(), i);
    }

    let mut formats: Vec<String> = Vec::new();
    for imp in &request.imp {
        if !imp_is_banner_only(imp) {
            continue;
        }
        let adslot_id = parse_ext_imp_yieldlab(imp)
            .map(|p| p.adslot_id)
            .unwrap_or_default();
        if adslot_id.is_empty() {
            continue;
        }
        if let Some(banner) = &imp.banner {
            let mut size_parts: Vec<String> = Vec::new();
            if let Some(formats) = &banner.format {
                for fmt in formats {
                    let w = fmt.w.unwrap_or(0);
                    let h = fmt.h.unwrap_or(0);
                    size_parts.push(format!("{}x{}", w, h));
                }
            }
            let sizes_str = size_parts.join("|");
            formats.push(format!("{}:{}", adslot_id, sizes_str));
        }
    }

    if formats.is_empty() {
        None
    } else {
        Some(formats.join(","))
    }
}

fn get_gdpr(request: &openrtb::BidRequest) -> (String, String) {
    let consent = request.user.as_ref()
        .and_then(|u| u.ext.as_ref())
        .and_then(|e| serde_json::from_value::<ExtUser>(e.clone()).ok())
        .map(|eu| eu.consent)
        .unwrap_or_default();

    let gdpr = request.regs.as_ref()
        .and_then(|r| r.ext.as_ref())
        .and_then(|e| serde_json::from_value::<ExtRegsWithDsa>(e.clone()).ok())
        .and_then(|er| er.gdpr)
        .map(|g| if g == 0 || g == 1 { g.to_string() } else { String::new() })
        .unwrap_or_default();

    (gdpr, consent)
}

fn get_dsa(request: &openrtb::BidRequest) -> Option<DsaRequest> {
    let ext = request.regs.as_ref()?.ext.as_ref()?;
    let ext_regs: ExtRegsWithDsa = serde_json::from_value(ext.clone()).ok()?;
    ext_regs.dsa
}

fn make_dsa_transparency_param(transparencies: &[DsaTransparencyRequest]) -> String {
    let mut parts: Vec<String> = Vec::new();
    for t in transparencies {
        if t.domain.is_empty() {
            continue;
        }
        let mut s = t.domain.clone();
        if !t.params.is_empty() {
            let param_str: Vec<String> = t.params.iter().map(|p| p.to_string()).collect();
            s.push('~');
            s.push_str(&param_str.join("_"));
        }
        parts.push(s);
    }
    parts.join("~~")
}

fn make_supply_chain(schain: &SupplyChain) -> String {
    if schain.nodes.is_empty() {
        return String::new();
    }
    let prefix = format!("{},{}", schain.ver, schain.complete);
    let mut sb = prefix;
    for node in &schain.nodes {
        let hp_str = node.hp.map(|v| v.to_string()).unwrap_or_default();
        let ext_str = match &node.ext {
            Some(v) => {
                let raw = serde_json::to_string(v).unwrap_or_default();
                url_encode(&raw)
            }
            None => String::new(),
        };
        let node_str = format!(
            "!{},{},{},{},{},{},{}",
            url_encode(&node.asi),
            url_encode(&node.sid),
            hp_str,
            url_encode(&node.rid),
            url_encode(&node.name),
            url_encode(&node.domain),
            ext_str,
        );
        sb.push_str(&node_str);
    }
    sb
}

fn unmarshal_supply_chain(request: &openrtb::BidRequest) -> Option<SupplyChain> {
    let source = request.source.as_ref()?;
    let ext = source.ext.as_ref()?;
    let schain_ext: ExtRequestPrebidSchain = serde_json::from_value(ext.clone()).ok()?;
    Some(schain_ext.schain)
}

fn make_endpoint_url(
    endpoint: &str,
    request: &openrtb::BidRequest,
    merged: &ExtImpYieldlab,
    all_params: &[ExtImpYieldlab],
) -> Result<String, BidderError> {
    // Parse base URL
    let mut uri = endpoint.trim_end_matches('/').to_string();
    if !merged.adslot_id.is_empty() {
        uri = format!("{}/{}", uri, merged.adslot_id);
    }

    let mut params: Vec<String> = Vec::new();
    params.push("content=json".to_string());
    params.push("pvid=true".to_string());
    params.push(format!("ts={}", cache_buster()));

    let targeting = make_targeting_values(&merged.targeting);
    params.push(format!("t={}", targeting));

    if let Some(formats) = make_formats(request, all_params) {
        params.push(format!("sizes={}", url_encode(&formats)));
    }

    if let Some(user) = &request.user {
        if let Some(buyer_uid) = &user.buyeruid {
            if !buyer_uid.is_empty() {
                params.push(format!("ids=ylid%3A{}", url_encode(buyer_uid)));
            }
        }
    }

    if let Some(device) = &request.device {
        if let Some(ifa) = &device.ifa {
            params.push(format!("yl_rtb_ifa={}", url_encode(ifa)));
        }
        if let Some(dt) = &device.devicetype {
            params.push(format!("yl_rtb_devicetype={}", dt));
        }
        if let Some(ct) = &device.connectiontype {
            params.push(format!("yl_rtb_connectiontype={}", ct));
        }
        if let Some(geo) = &device.geo {
            if let Some(lat) = geo.lat {
                params.push(format!("lat={}", lat));
            }
            if let Some(lon) = geo.lon {
                params.push(format!("lon={}", lon));
            }
        }
    }

    if let Some(app) = &request.app {
        if let Some(name) = &app.name {
            params.push(format!("pubappname={}", url_encode(name)));
        }
        if let Some(bundle) = &app.bundle {
            params.push(format!("pubbundlename={}", url_encode(bundle)));
        }
    }

    let (gdpr, consent) = get_gdpr(request);
    if !gdpr.is_empty() {
        params.push(format!("gdpr={}", gdpr));
    }
    if !consent.is_empty() {
        params.push(format!("gdpr_consent={}", url_encode(&consent)));
    }

    if request.source.as_ref().and_then(|s| s.ext.as_ref()).is_some() {
        if let Some(schain) = unmarshal_supply_chain(request) {
            let schain_val = make_supply_chain(&schain);
            if !schain_val.is_empty() {
                params.push(format!("schain={}", url_encode(&schain_val)));
            }
        }
    }

    if let Some(dsa) = get_dsa(request) {
        if let Some(required) = dsa.required {
            params.push(format!("dsarequired={}", required));
        }
        if let Some(pub_render) = dsa.pub_render {
            params.push(format!("dsapubrender={}", pub_render));
        }
        if let Some(data_to_pub) = dsa.data_to_pub {
            params.push(format!("dsadatatopub={}", data_to_pub));
        }
        if !dsa.transparency.is_empty() {
            let tp = make_dsa_transparency_param(&dsa.transparency);
            if !tp.is_empty() {
                params.push(format!("dsatransparency={}", url_encode(&tp)));
            }
        }
    }

    Ok(format!("{}?{}", uri, params.join("&")))
}

fn make_ad_source_url(
    request: &openrtb::BidRequest,
    ext: &ExtImpYieldlab,
    bid: &YieldlabBidResponse,
) -> String {
    let mut params: Vec<String> = Vec::new();
    params.push(format!("ts={}", cache_buster()));
    params.push(format!("id={}", url_encode(&ext.ext_id)));
    params.push(format!("pvid={}", url_encode(&bid.pvid)));

    if let Some(user) = &request.user {
        if let Some(buyer_uid) = &user.buyeruid {
            if !buyer_uid.is_empty() {
                params.push(format!("ids=ylid%3A{}", url_encode(buyer_uid)));
            }
        }
    }

    let (gdpr, consent) = get_gdpr(request);
    if !gdpr.is_empty() && !consent.is_empty() {
        params.push(format!("gdpr={}", gdpr));
        params.push(format!("gdpr_consent={}", url_encode(&consent)));
    }

    format!(
        "https://ad.yieldlab.net/d/{}/{}/{}?{}",
        ext.adslot_id,
        ext.supply_id,
        bid.adsize,
        params.join("&")
    )
}

fn make_banner_ad_source(
    request: &openrtb::BidRequest,
    ext: &ExtImpYieldlab,
    bid: &YieldlabBidResponse,
) -> String {
    let url = make_ad_source_url(request, ext, bid);
    format!("<script src=\"{}\"></script>", url)
}

fn make_vast(
    request: &openrtb::BidRequest,
    ext: &ExtImpYieldlab,
    bid: &YieldlabBidResponse,
) -> String {
    let url = make_ad_source_url(request, ext, bid);
    format!(
        "<VAST version=\"2.0\"><Ad id=\"{}\"><Wrapper><AdSystem>Yieldlab</AdSystem><VASTAdTagURI><![CDATA[ {} ]]></VASTAdTagURI><Impression></Impression><Creatives></Creatives></Wrapper></Ad></VAST>",
        ext.adslot_id,
        url
    )
}

fn make_creative_id(ext: &ExtImpYieldlab, bid: &YieldlabBidResponse) -> String {
    format!("{}{}{}", ext.adslot_id, bid.pid, get_week())
}

impl Bidder for YieldlabAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::BadInput("no impressions given".to_string())]);
        }

        // Parse all imp ext params
        let all_params: Vec<ExtImpYieldlab> = request.imp.iter()
            .filter_map(|imp| parse_ext_imp_yieldlab(imp))
            .collect();

        if all_params.is_empty() {
            return (vec![], vec![BidderError::BadInput("no valid yieldlab params found".to_string())]);
        }

        // Merge all adslot IDs and targeting
        let mut adslot_ids: Vec<String> = Vec::new();
        let mut targeting: HashMap<String, String> = HashMap::new();
        for p in &all_params {
            if !p.adslot_id.is_empty() {
                adslot_ids.push(p.adslot_id.clone());
            }
            for (k, v) in &p.targeting {
                targeting.insert(k.clone(), v.clone());
            }
        }

        let merged = ExtImpYieldlab {
            adslot_id: adslot_ids.join(","),
            targeting,
            ..Default::default()
        };

        let bid_url = match make_endpoint_url(&self.endpoint, request, &merged, &all_params) {
            Ok(u) => u,
            Err(e) => return (vec![], vec![e]),
        };

        let mut headers = HashMap::new();
        headers.insert("Accept".to_string(), "application/json".to_string());

        if let Some(site) = &request.site {
            if let Some(page) = &site.page {
                headers.insert("Referer".to_string(), page.clone());
            }
        }
        if let Some(device) = &request.device {
            if let Some(ua) = &device.ua {
                headers.insert("User-Agent".to_string(), ua.clone());
            }
            if let Some(ip) = &device.ip {
                headers.insert("X-Forwarded-For".to_string(), ip.clone());
            }
        }
        if let Some(user) = &request.user {
            if let Some(buyer_uid) = &user.buyeruid {
                if !buyer_uid.is_empty() {
                    headers.insert("Cookie".to_string(), format!("id={}", buyer_uid));
                }
            }
        }

        (vec![RequestData {
            method: "GET".to_string(),
            uri: bid_url,
            body: vec![],
            headers,
            imp_ids: get_imp_ids(&request.imp),
        }], vec![])
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 {
            return Ok(BidderResponse::new());
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "failed to resolve bids from yieldlab response: Unexpected response code {}",
                response.status_code
            ))]);
        }

        let yieldlab_bids: Vec<YieldlabBidResponse> = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(format!(
                "failed to parse bids response from yieldlab: {}", e
            ))])?;

        // Re-parse all params from the internal request
        let all_params: Vec<ExtImpYieldlab> = internal.imp.iter()
            .filter_map(|imp| parse_ext_imp_yieldlab(imp))
            .collect();

        // Build adslot_id -> Imp map for banner/video imps
        let mut adslot_to_imp: HashMap<String, &openrtb::Imp> = HashMap::new();
        for imp in &internal.imp {
            if imp.video.is_some() || imp.banner.is_some() {
                if let Some(params) = parse_ext_imp_yieldlab(imp) {
                    adslot_to_imp.insert(params.adslot_id, imp);
                }
            }
        }

        let mut result = BidderResponse::with_capacity(yieldlab_bids.len());
        result.currency = "EUR".to_string();
        let mut bid_errors: Vec<BidderError> = Vec::new();

        for bid in &yieldlab_bids {
            let bid_id_str = bid.id.to_string();

            // Find the matching imp
            let imp = match adslot_to_imp.get(&bid_id_str) {
                Some(i) => *i,
                None => continue, // skip if no matching imp
            };

            // Find the matching params
            let req_params = match all_params.iter().find(|p| p.adslot_id == bid_id_str) {
                Some(p) => p,
                None => {
                    bid_errors.push(BidderError::BadServerResponse(format!(
                        "failed to find yieldlab request for adslotID {}", bid.id
                    )));
                    continue;
                }
            };

            // Parse adsize
            let (w, h) = split_size(&bid.adsize).unwrap_or((0, 0));

            // Build ext JSON with DSA if present
            let ext_json = if let Some(dsa) = &bid.dsa {
                match serde_json::to_value(ResponseExtWithDsa { dsa: dsa.clone() }) {
                    Ok(v) => Some(v),
                    Err(e) => {
                        bid_errors.push(BidderError::BadServerResponse(format!(
                            "failed to make JSON for seatbid.bid.ext for adslotID {}: {}", bid.id, e
                        )));
                        continue;
                    }
                }
            } else {
                None
            };

            let mut rtb_bid = openrtb::Bid {
                id: bid_id_str.clone(),
                impid: imp.id.clone(),
                price: bid.price as f64 / 100.0,
                crid: Some(make_creative_id(req_params, bid)),
                dealid: Some(bid.pid.to_string()),
                w: Some(w),
                h: Some(h),
                adomain: Some(vec![bid.advertiser.clone()]),
                ext: ext_json,
                ..Default::default()
            };

            let bid_type = if imp.video.is_some() {
                rtb_bid.nurl = Some(make_ad_source_url(internal, req_params, bid));
                rtb_bid.adm = Some(make_vast(internal, req_params, bid));
                BidType::Video
            } else if imp.banner.is_some() {
                rtb_bid.adm = Some(make_banner_ad_source(internal, req_params, bid));
                BidType::Banner
            } else {
                // Yieldlab adapter does not support audio or native
                continue;
            };

            result.bids.push(TypedBid::new(rtb_bid, bid_type));
        }

        if !bid_errors.is_empty() {
            return Err(bid_errors);
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_req() -> openrtb::BidRequest {
        openrtb::BidRequest {
            id: "r".to_string(),
            imp: vec![openrtb::Imp {
                id: "i1".to_string(),
                banner: Some(openrtb::Banner {
                    w: Some(300),
                    h: Some(250),
                    format: Some(vec![openrtb::Format {
                        w: Some(300),
                        h: Some(250),
                        ..Default::default()
                    }]),
                    ..Default::default()
                }),
                ext: Some(serde_json::json!({
                    "bidder": {"adslotId": "1111", "supplyId": "2222"}
                })),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn test_yieldlab_url_and_headers() {
        let adapter = YieldlabAdapter::new("https://ad.yieldlab.net".to_string());
        let (reqs, errs) = adapter.make_requests(&make_req(), &ExtraRequestInfo::default());
        assert!(errs.is_empty(), "errs: {:?}", errs);
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].method, "GET");
        assert!(
            reqs[0].uri.starts_with("https://ad.yieldlab.net/1111?"),
            "uri: {}",
            reqs[0].uri
        );
        assert_eq!(reqs[0].headers.get("Accept").unwrap(), "application/json");
    }
}
