use std::collections::HashMap;

use crate::{
    Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid,
    get_imp_ids,
};
use openrtb_ext::{BidType, ExtBidPrebidVideo, FledgeAuctionConfig};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub struct IxAdapter {
    pub endpoint: String,
}

impl IxAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

/// IX imp ext bidder params
#[derive(Debug, Default, Deserialize)]
struct ExtImpIx {
    #[serde(rename = "siteId", default)]
    site_id: String,
    #[serde(default)]
    sid: String,
}

#[derive(Debug, Deserialize)]
struct ImpExt {
    bidder: ExtImpIx,
}

/// IX response extension for FLEDGE auction configs
#[derive(Debug, Default, Deserialize)]
struct IxRespExt {
    #[serde(rename = "protectedAudienceAuctionConfigs", default)]
    auction_configs: Vec<IxAuctionConfig>,
}

#[derive(Debug, Deserialize)]
struct IxAuctionConfig {
    #[serde(rename = "bidId", default)]
    bid_id: String,
    #[serde(default)]
    config: Option<Value>,
}

/// Bid ext for determining type from prebid.type
#[derive(Debug, Default, Deserialize)]
struct BidExt {
    prebid: Option<BidExtPrebid>,
}

#[derive(Debug, Default, Deserialize)]
struct BidExtPrebid {
    #[serde(rename = "type", default)]
    bid_type: String,
    video: Option<BidExtPrebidVideo>,
}

#[derive(Debug, Default, Deserialize)]
struct BidExtPrebidVideo {
    #[serde(default)]
    duration: i32,
    #[serde(rename = "primaryCategory", default)]
    primary_category: String,
}

fn parse_bid_type_from_str(s: &str) -> Option<BidType> {
    match s {
        "banner" => Some(BidType::Banner),
        "video" => Some(BidType::Video),
        "native" => Some(BidType::Native),
        "audio" => Some(BidType::Audio),
        _ => None,
    }
}

/// Determine media type for a bid, checking mtype field first, then ext.prebid.type, then request imp map
fn get_media_type_for_bid(
    bid: &openrtb::Bid,
    imp_media_map: &HashMap<String, BidType>,
) -> Result<BidType, BidderError> {
    // Check ext.prebid.type
    if let Some(ext) = &bid.ext {
        if let Ok(bid_ext) = serde_json::from_value::<BidExt>(ext.clone()) {
            if let Some(prebid) = &bid_ext.prebid {
                if !prebid.bid_type.is_empty() {
                    if let Some(t) = parse_bid_type_from_str(&prebid.bid_type) {
                        return Ok(t);
                    }
                }
            }
        }
    }

    // Fall back to imp media type map
    if let Some(t) = imp_media_map.get(&bid.impid) {
        return Ok(t.clone());
    }

    Err(BidderError::BadServerResponse(format!(
        "unmatched impression id: {}",
        bid.impid
    )))
}

impl Bidder for IxAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut unique_site_ids: HashMap<String, ()> = HashMap::new();
        let mut filtered_imps = Vec::new();

        let mut req_copy = request.clone();

        for imp in &request.imp {
            let imp_ext: ImpExt = match imp
                .ext
                .as_ref()
                .and_then(|e| serde_json::from_value(e.clone()).ok())
            {
                Some(e) => e,
                None => {
                    errs.push(BidderError::BadInput(
                        "Failed to parse ix imp ext".to_string(),
                    ));
                    continue;
                }
            };

            let ix_ext = &imp_ext.bidder;

            if !ix_ext.site_id.is_empty() {
                unique_site_ids.insert(ix_ext.site_id.clone(), ());
            }

            let mut imp_copy = imp.clone();

            // Move sid from bidder ext into imp.ext.sid if present
            if !ix_ext.sid.is_empty() {
                if let Some(ext_val) = &imp_copy.ext {
                    if let Ok(mut ext_map) = serde_json::from_value::<serde_json::Map<String, Value>>(ext_val.clone()) {
                        ext_map.insert("sid".to_string(), Value::String(ix_ext.sid.clone()));
                        imp_copy.ext = Some(Value::Object(ext_map));
                    }
                }
            }

            // Normalize banner format
            if let Some(banner) = &imp_copy.banner {
                let mut banner_copy = banner.clone();
                if banner_copy.format.as_ref().map_or(true, |f| f.is_empty()) {
                    if let (Some(w), Some(h)) = (banner_copy.w, banner_copy.h) {
                        banner_copy.format = Some(vec![openrtb::Format {
                            w: Some(w),
                            h: Some(h),
                            wratio: None,
                            hratio: None,
                            wmin: None,
                            ext: None,
                        }]);
                    }
                }
                if banner_copy.format.as_ref().map_or(false, |f| f.len() == 1) {
                    if let Some(formats) = &banner_copy.format {
                        banner_copy.w = formats[0].w;
                        banner_copy.h = formats[0].h;
                    }
                }
                imp_copy.banner = Some(banner_copy);
            }

            filtered_imps.push(imp_copy);
        }

        req_copy.imp = filtered_imps;

        // Set publisher ID from site IDs
        let site_ids: Vec<String> = unique_site_ids.keys().cloned().collect();

        if let Some(site) = &req_copy.site {
            let mut site_copy = site.clone();
            let mut pub_copy = site_copy.publisher.clone().unwrap_or_default();
            if site_ids.len() == 1 {
                pub_copy.id = Some(site_ids[0].clone());
            }
            site_copy.publisher = Some(pub_copy);
            req_copy.site = Some(site_copy);
        }

        if let Some(app) = &req_copy.app {
            let mut app_copy = app.clone();
            let mut pub_copy = app_copy.publisher.clone().unwrap_or_default();
            if site_ids.len() == 1 {
                pub_copy.id = Some(site_ids[0].clone());
            }
            app_copy.publisher = Some(pub_copy);
            req_copy.app = Some(app_copy);
        }

        if req_copy.imp.is_empty() {
            return (vec![], errs);
        }

        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => {
                errs.push(BidderError::BadInput(e.to_string()));
                return (vec![], errs);
            }
        };

        let mut headers = HashMap::new();
        headers.insert(
            "Content-Type".to_string(),
            "application/json;charset=utf-8".to_string(),
        );
        headers.insert("Accept".to_string(), "application/json".to_string());

        let imp_ids = get_imp_ids(&req_copy.imp);

        (
            vec![RequestData {
                method: "POST".to_string(),
                uri: self.endpoint.clone(),
                body,
                headers,
                imp_ids,
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

        let bid_response: openrtb::BidResponse =
            serde_json::from_slice(&response.body)
                .map_err(|e| vec![BidderError::BadServerResponse(format!("JSON parsing error: {}", e))])?;

        // Build imp media type map
        let mut imp_media_map: HashMap<String, BidType> = HashMap::new();
        for imp in &internal.imp {
            let bid_type = if imp.banner.is_some() {
                BidType::Banner
            } else if imp.video.is_some() {
                BidType::Video
            } else if imp.native.is_some() {
                BidType::Native
            } else if imp.audio.is_some() {
                BidType::Audio
            } else {
                continue;
            };
            imp_media_map.insert(imp.id.clone(), bid_type);
        }

        let mut result = BidderResponse::with_capacity(0);
        if let Some(cur) = &bid_response.cur {
            result.currency = cur.clone();
        }

        let mut errs = Vec::new();

        for seat_bid in bid_response.seatbid {
            for bid in seat_bid.bid {
                let bid_type = match get_media_type_for_bid(&bid, &imp_media_map) {
                    Ok(t) => t,
                    Err(e) => {
                        errs.push(e);
                        continue;
                    }
                };

                // Extract video ext if bid type is video
                let bid_video = if bid_type == BidType::Video {
                    bid.ext.as_ref().and_then(|ext| {
                        serde_json::from_value::<BidExt>(ext.clone()).ok()
                    }).and_then(|be| be.prebid).and_then(|p| p.video).map(|v| {
                        ExtBidPrebidVideo {
                            duration: v.duration,
                            primary_category: v.primary_category,
                        }
                    })
                } else {
                    None
                };

                let mut typed_bid = TypedBid::new(bid, bid_type);
                typed_bid.bid_video = bid_video;
                result.bids.push(typed_bid);
            }
        }

        // Parse FLEDGE auction configs from response ext
        if let Some(ext) = &bid_response.ext {
            if let Ok(resp_ext) = serde_json::from_value::<IxRespExt>(ext.clone()) {
                for config in resp_ext.auction_configs {
                    if let Some(cfg) = config.config {
                        result.fledge_auction_configs.push(FledgeAuctionConfig {
                            impid: config.bid_id,
                            bidder: None,
                            adapter: None,
                            config: cfg,
                        });
                    }
                }
            }
        }

        if !errs.is_empty() {
            return Err(errs);
        }

        Ok(result)
    }
}
