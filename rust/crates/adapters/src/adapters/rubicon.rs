use std::collections::HashMap;

use crate::{
    Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid,
    get_imp_ids,
};
use openrtb_ext::BidType;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub struct RubiconAdapter {
    pub endpoint: String,
    pub xapi_username: String,
    pub xapi_password: String,
}

impl RubiconAdapter {
    pub fn new(endpoint: String, xapi_username: String, xapi_password: String) -> Self {
        Self {
            endpoint,
            xapi_username,
            xapi_password,
        }
    }
}

/// Rubicon imp ext bidder params
#[derive(Debug, Default, Deserialize)]
struct ExtImpRubicon {
    #[serde(rename = "accountId", default)]
    account_id: Value,
    #[serde(rename = "siteId", default)]
    site_id: Value,
    #[serde(rename = "zoneId", default)]
    zone_id: Value,
    #[serde(default)]
    inventory: Option<Value>,
    #[serde(default)]
    visitor: Option<Value>,
    #[serde(default)]
    video: Option<RubiconVideoExt>,
}

#[derive(Debug, Default, Deserialize)]
struct RubiconVideoExt {
    #[serde(rename = "skip", default)]
    skip: i32,
    #[serde(rename = "skipdelay", default)]
    skip_delay: i32,
    #[serde(rename = "videoSizeId", default)]
    video_size_id: i32,
}

/// Wrapper for imp.ext containing the bidder ext
#[derive(Debug, Deserialize)]
struct ImpExt {
    bidder: ExtImpRubicon,
    #[serde(default)]
    gpid: String,
    #[serde(default)]
    tid: String,
}

fn value_to_i64(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => n.as_i64(),
        Value::String(s) => s.parse::<i64>().ok(),
        _ => None,
    }
}

fn add_basic_auth(headers: &mut HashMap<String, String>, username: &str, password: &str) {
    use std::fmt::Write;
    let credentials = format!("{}:{}", username, password);
    let encoded = base64_encode(credentials.as_bytes());
    headers.insert("Authorization".to_string(), format!("Basic {}", encoded));
}

fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    let mut i = 0;
    while i < data.len() {
        let b0 = data[i] as u32;
        let b1 = if i + 1 < data.len() { data[i + 1] as u32 } else { 0 };
        let b2 = if i + 2 < data.len() { data[i + 2] as u32 } else { 0 };
        result.push(CHARS[((b0 >> 2) & 0x3f) as usize] as char);
        result.push(CHARS[(((b0 << 4) | (b1 >> 4)) & 0x3f) as usize] as char);
        if i + 1 < data.len() {
            result.push(CHARS[(((b1 << 2) | (b2 >> 6)) & 0x3f) as usize] as char);
        } else {
            result.push('=');
        }
        if i + 2 < data.len() {
            result.push(CHARS[(b2 & 0x3f) as usize] as char);
        } else {
            result.push('=');
        }
        i += 3;
    }
    result
}

impl Bidder for RubiconAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut requests = Vec::new();

        let mut headers = HashMap::new();
        headers.insert(
            "Content-Type".to_string(),
            "application/json;charset=utf-8".to_string(),
        );
        headers.insert("Accept".to_string(), "application/json".to_string());
        headers.insert("User-Agent".to_string(), "prebid-server/1.0".to_string());
        add_basic_auth(&mut headers, &self.xapi_username, &self.xapi_password);

        for imp in &request.imp {
            let imp_ext: ImpExt = match imp
                .ext
                .as_ref()
                .and_then(|e| serde_json::from_value(e.clone()).ok())
            {
                Some(e) => e,
                None => {
                    errs.push(BidderError::BadInput(
                        "Failed to parse rubicon imp ext".to_string(),
                    ));
                    continue;
                }
            };

            let rubicon_ext = &imp_ext.bidder;

            let site_id = match value_to_i64(&rubicon_ext.site_id) {
                Some(v) => v,
                None => {
                    errs.push(BidderError::BadInput(
                        "Invalid siteId in rubicon ext".to_string(),
                    ));
                    continue;
                }
            };

            let zone_id = match value_to_i64(&rubicon_ext.zone_id) {
                Some(v) => v,
                None => {
                    errs.push(BidderError::BadInput(
                        "Invalid zoneId in rubicon ext".to_string(),
                    ));
                    continue;
                }
            };

            let account_id = match value_to_i64(&rubicon_ext.account_id) {
                Some(v) => v,
                None => {
                    errs.push(BidderError::BadInput(
                        "Invalid accountId in rubicon ext".to_string(),
                    ));
                    continue;
                }
            };

            // Determine bid type (video takes priority)
            let is_video = imp.video.is_some() && imp.banner.is_none();
            let imp_type = if is_video {
                BidType::Video
            } else if imp.banner.is_some() {
                BidType::Banner
            } else {
                BidType::Native
            };

            let mut imp_copy = imp.clone();

            // Build rubicon imp ext
            let imp_rp_ext = serde_json::json!({
                "rp": {
                    "zone_id": zone_id,
                    "track": { "mint": "", "mint_version": "" }
                },
                "gpid": imp_ext.gpid,
                "tid": imp_ext.tid,
            });
            imp_copy.ext = Some(imp_rp_ext);

            let secure: i32 = 1;
            imp_copy.secure = Some(secure);

            // Build the per-imp request
            let pub_ext = serde_json::json!({
                "rp": { "account_id": account_id }
            });

            let mut req_copy = request.clone();
            req_copy.imp = vec![imp_copy];
            req_copy.cur = None;
            req_copy.ext = None;

            // Set site or app with rubicon RP fields
            if let Some(site) = &request.site {
                let mut site_copy = site.clone();
                site_copy.ext = Some(serde_json::json!({
                    "rp": { "site_id": site_id }
                }));
                site_copy.publisher = Some(openrtb::Publisher {
                    ext: Some(pub_ext.clone()),
                    ..Default::default()
                });
                req_copy.site = Some(site_copy);
            } else if let Some(app) = &request.app {
                let mut app_copy = app.clone();
                app_copy.ext = Some(serde_json::json!({
                    "rp": { "site_id": site_id }
                }));
                app_copy.publisher = Some(openrtb::Publisher {
                    ext: Some(pub_ext.clone()),
                    ..Default::default()
                });
                req_copy.app = Some(app_copy);
            }

            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(BidderError::BadInput(e.to_string()));
                    continue;
                }
            };

            let imp_ids = get_imp_ids(&req_copy.imp);

            requests.push(RequestData {
                method: "POST".to_string(),
                uri: self.endpoint.clone(),
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

        let bid_response: openrtb::BidResponse =
            serde_json::from_slice(&response.body)
                .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(5);

        for seat_bid in bid_response.seatbid {
            for bid in seat_bid.bid {
                // Rubicon always sends one imp per request; determine type from ext or default to banner
                let bid_type = if let Some(ext) = &bid.ext {
                    if let Some(media_type) = ext.get("prebid").and_then(|p| p.get("type")).and_then(|t| t.as_str()) {
                        match media_type {
                            "video" => BidType::Video,
                            "native" => BidType::Native,
                            _ => BidType::Banner,
                        }
                    } else {
                        BidType::Banner
                    }
                } else {
                    BidType::Banner
                };

                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }

        Ok(result)
    }
}
