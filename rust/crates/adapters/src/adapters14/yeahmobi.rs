use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::{BidType, ExtBidPrebidVideo};
use serde::{Deserialize, Serialize};

pub struct YeahmobiAdapter {
    pub endpoint: String,
}

impl YeahmobiAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

/// Extension from imp.ext.bidder for Yeahmobi
#[derive(Debug, Default, Deserialize)]
struct ExtImpYeahmobi {
    #[serde(rename = "zoneId", default)]
    zone_id: String,
}

/// Bid ext video info
#[derive(Debug, Default, Deserialize)]
struct YeahmobiBidExtVideo {
    #[serde(rename = "duration")]
    duration: Option<i32>,
}

#[derive(Debug, Default, Deserialize)]
struct YeahmobiBidExt {
    #[serde(rename = "video")]
    video_creative_info: Option<YeahmobiBidExtVideo>,
}

/// URL-encode a string (percent-encoding)
fn percent_encode(s: &str) -> String {
    let mut encoded = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9'
            | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            b => {
                encoded.push_str(&format!("%{:02X}", b));
            }
        }
    }
    encoded
}

fn get_yeahmobi_ext(request: &openrtb::BidRequest) -> Result<ExtImpYeahmobi, Vec<BidderError>> {
    let mut errors = Vec::new();
    for imp in &request.imp {
        let bidder_val = match imp.ext.as_ref().and_then(|e| e.get("bidder")) {
            Some(v) => v.clone(),
            None => {
                errors.push(BidderError::BadInput("missing bidder ext".to_string()));
                continue;
            }
        };
        match serde_json::from_value::<ExtImpYeahmobi>(bidder_val) {
            Ok(ext) => return Ok(ext),
            Err(e) => {
                errors.push(BidderError::BadInput(e.to_string()));
                continue;
            }
        }
    }
    Err(errors)
}

fn get_endpoint(base_endpoint: &str, zone_id: &str) -> String {
    // Go code: macros.ResolveMacros with Host = "gw-" + url.QueryEscape(ext.ZoneId) + "-bid.yeahtargeter.com"
    let host = format!("gw-{}-bid.yeahtargeter.com", percent_encode(zone_id));
    base_endpoint.replace("{{.Host}}", &host)
}

fn get_bid_type(imp_id: &str, imps: &[openrtb::Imp]) -> BidType {
    for imp in imps {
        if imp.id == imp_id {
            if imp.banner.is_some() {
                return BidType::Banner;
            }
            if imp.video.is_some() {
                return BidType::Video;
            }
            if imp.native.is_some() {
                return BidType::Native;
            }
        }
    }
    BidType::Banner
}

/// Transform native request: wrap in {"native": ...} if not already wrapped
fn transform(request: &mut openrtb::BidRequest) {
    for imp in &mut request.imp {
        if let Some(native) = &imp.native {
            let req_str = match &native.request {
                Some(s) if !s.is_empty() => s.clone(),
                _ => continue,
            };
            let native_request: serde_json::Value = match serde_json::from_str(&req_str) {
                Ok(v) => v,
                Err(_) => continue,
            };
            // Check if already wrapped
            if native_request.get("native").is_some() {
                continue;
            }
            let mut wrapped = serde_json::Map::new();
            wrapped.insert("native".to_string(), native_request);
            let wrapped_str = match serde_json::to_string(&serde_json::Value::Object(wrapped)) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let mut native_copy = native.clone();
            native_copy.request = Some(wrapped_str);
            imp.native = Some(native_copy);
        }
    }
}

impl Bidder for YeahmobiAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let yeahmobi_ext = match get_yeahmobi_ext(request) {
            Ok(ext) => ext,
            Err(errs) => return (vec![], errs),
        };

        let endpoint = get_endpoint(&self.endpoint, &yeahmobi_ext.zone_id);

        // Clone and transform the request
        let mut req_copy = request.clone();
        transform(&mut req_copy);

        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());

        (
            vec![RequestData {
                method: "POST".to_string(),
                uri: endpoint,
                body,
                headers,
                imp_ids: get_imp_ids(&request.imp),
            }],
            vec![],
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
                "Unexpected status code: {}.",
                response.status_code
            ))]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected status code: {}.",
                response.status_code
            ))]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(1);

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let media_type = get_bid_type(&bid.impid, &internal.imp);
                let mut typed_bid = TypedBid::new(bid.clone(), media_type);
                typed_bid.bid_video = Some(ExtBidPrebidVideo { duration: 0, primary_category: String::new() });

                if let Some(ext_val) = &bid.ext {
                    if let Ok(bid_ext) = serde_json::from_value::<YeahmobiBidExt>(ext_val.clone()) {
                        if let Some(video_info) = bid_ext.video_creative_info {
                            if let Some(dur) = video_info.duration {
                                if let Some(bv) = &mut typed_bid.bid_video {
                                    bv.duration = dur;
                                }
                            }
                        }
                    }
                }

                result.bids.push(typed_bid);
            }
        }

        Ok(result)
    }
}
