use std::collections::HashMap;

use crate::{
    Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid,
    get_imp_ids, get_bid_type_from_imp,
};
use openrtb_ext::{BidType, ExtBidPrebidVideo, FledgeAuctionConfig};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[allow(dead_code)]
const MAX_IMPS_PUBMATIC: usize = 30;

pub struct PubmaticAdapter {
    pub endpoint: String,
}

impl PubmaticAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

/// Pubmatic imp ext bidder params
#[derive(Debug, Default, Deserialize)]
struct ExtImpPubmatic {
    #[serde(rename = "publisherId", default)]
    publisher_id: String,
    #[serde(rename = "adSlot", default)]
    ad_slot: String,
    #[serde(rename = "pmzoneid", default)]
    pm_zone_id: String,
    #[serde(rename = "pmZoneId", default)]
    pm_zone_id_alt: String,
    #[serde(default)]
    #[allow(dead_code)]
    keywords: Option<Vec<PubmaticKeyword>>,
    #[serde(default)]
    wrapper: Option<PubmaticWrapperExt>,
}

#[derive(Debug, Default, Deserialize)]
struct PubmaticKeyword {
    #[allow(dead_code)]
    key: String,
    #[allow(dead_code)]
    value: Vec<String>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct PubmaticWrapperExt {
    #[serde(skip_serializing_if = "is_zero", default)]
    profile: i32,
    #[serde(skip_serializing_if = "is_zero", default)]
    version: i32,
}

fn is_zero(v: &i32) -> bool {
    *v == 0
}

#[derive(Debug, Deserialize)]
struct ImpExt {
    bidder: ExtImpPubmatic,
}

/// Pubmatic bid ext for video creative info and deal priority
#[derive(Debug, Default, Deserialize)]
struct PubmaticBidExt {
    video: Option<PubmaticBidExtVideo>,
    #[serde(rename = "prebiddealpriority", default)]
    deal_priority: i32,
}

#[derive(Debug, Default, Deserialize)]
struct PubmaticBidExtVideo {
    duration: Option<i32>,
}

/// Pubmatic response ext for FLEDGE
#[derive(Debug, Default, Deserialize)]
struct PubmaticRespExt {
    #[serde(rename = "fledge_auction_configs", default)]
    fledge_auction_configs: HashMap<String, Value>,
}

/// Validate and parse adSlot string. Returns (tagid, width, height)
fn parse_ad_slot(ad_slot: &str) -> Result<(String, Option<i64>, Option<i64>), BidderError> {
    let s = ad_slot.trim();
    if s.is_empty() {
        return Ok((String::new(), None, None));
    }
    if !s.contains('@') {
        return Ok((s.to_string(), None, None));
    }
    let parts: Vec<&str> = s.splitn(2, '@').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        return Err(BidderError::BadInput(format!("Invalid adSlot: {}", s)));
    }
    let tag_id = parts[0].trim().to_string();
    let size_str = parts[1].to_lowercase();
    let size_parts: Vec<&str> = size_str.splitn(2, 'x').collect();
    if size_parts.len() != 2 {
        return Err(BidderError::BadInput(format!("Invalid size in adSlot: {}", s)));
    }
    let width = size_parts[0].trim().parse::<i64>()
        .map_err(|_| BidderError::BadInput(format!("Invalid width in adSlot: {}", s)))?;
    // Height may have `:ratio` suffix, ignore that
    let height_str = size_parts[1].splitn(2, ':').next().unwrap_or("").trim();
    let height = height_str.parse::<i64>()
        .map_err(|_| BidderError::BadInput(format!("Invalid height in adSlot: {}", s)))?;
    Ok((tag_id, Some(width), Some(height)))
}

impl Bidder for PubmaticAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut pub_id = String::new();
        let mut wrapper_ext: Option<PubmaticWrapperExt> = None;
        let mut valid_imps = Vec::new();

        for imp in &request.imp {
            let imp_ext: ImpExt = match imp
                .ext
                .as_ref()
                .and_then(|e| serde_json::from_value(e.clone()).ok())
            {
                Some(e) => e,
                None => {
                    errs.push(BidderError::BadInput(
                        "Failed to parse pubmatic imp ext".to_string(),
                    ));
                    continue;
                }
            };

            let bidder_ext = &imp_ext.bidder;

            if pub_id.is_empty() && !bidder_ext.publisher_id.is_empty() {
                pub_id = bidder_ext.publisher_id.clone();
            }

            // Collect wrapper ext from first imp that has it
            if wrapper_ext.is_none() {
                if let Some(w) = &bidder_ext.wrapper {
                    wrapper_ext = Some(PubmaticWrapperExt {
                        profile: w.profile,
                        version: w.version,
                    });
                }
            }

            let mut imp_copy = imp.clone();

            // Parse and validate ad slot
            if !bidder_ext.ad_slot.is_empty() {
                match parse_ad_slot(&bidder_ext.ad_slot) {
                    Ok((tag_id, w, h)) => {
                        if !tag_id.is_empty() {
                            imp_copy.tagid = Some(tag_id);
                        }
                        // Set banner size if parsed
                        if let (Some(width), Some(height)) = (w, h) {
                            if let Some(banner) = &imp_copy.banner {
                                let mut banner_copy = banner.clone();
                                if banner_copy.w.is_none() || banner_copy.h.is_none() {
                                    banner_copy.w = Some(width as i32);
                                    banner_copy.h = Some(height as i32);
                                }
                                imp_copy.banner = Some(banner_copy);
                            }
                        }
                    }
                    Err(e) => {
                        errs.push(e);
                        continue;
                    }
                }
            }

            // Build imp ext with dctr (key_val) targeting
            let zone_id = if !bidder_ext.pm_zone_id_alt.is_empty() {
                &bidder_ext.pm_zone_id_alt
            } else {
                &bidder_ext.pm_zone_id
            };

            let mut new_ext = serde_json::Map::new();
            if !zone_id.is_empty() {
                new_ext.insert("pmZoneId".to_string(), Value::String(zone_id.clone()));
            }
            imp_copy.ext = if new_ext.is_empty() {
                None
            } else {
                Some(Value::Object(new_ext))
            };

            valid_imps.push(imp_copy);
        }

        if valid_imps.is_empty() {
            return (vec![], errs);
        }

        // Build request-level ext with wrapper
        let req_ext = if let Some(w) = wrapper_ext {
            serde_json::json!({
                "wrapper": {
                    "profile": w.profile,
                    "version": w.version,
                }
            })
        } else {
            serde_json::json!({})
        };

        let mut req_copy = request.clone();
        req_copy.imp = valid_imps;
        req_copy.ext = Some(req_ext);

        // Set publisher ID in site or app
        if let Some(site) = &req_copy.site {
            let mut site_copy = site.clone();
            let mut pub_copy = site_copy.publisher.clone().unwrap_or_default();
            pub_copy.id = Some(pub_id.clone());
            site_copy.publisher = Some(pub_copy);
            req_copy.site = Some(site_copy);
        } else if let Some(app) = &req_copy.app {
            let mut app_copy = app.clone();
            let mut pub_copy = app_copy.publisher.clone().unwrap_or_default();
            pub_copy.id = Some(pub_id.clone());
            app_copy.publisher = Some(pub_copy);
            req_copy.app = Some(app_copy);
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
                .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(5);

        if let Some(cur) = &bid_response.cur {
            if !cur.is_empty() {
                result.currency = cur.clone();
            }
        }

        // Parse FLEDGE auction configs
        if let Some(ext) = &bid_response.ext {
            if let Ok(resp_ext) = serde_json::from_value::<PubmaticRespExt>(ext.clone()) {
                for (imp_id, config) in resp_ext.fledge_auction_configs {
                    result.fledge_auction_configs.push(FledgeAuctionConfig {
                        impid: imp_id,
                        bidder: None,
                        adapter: None,
                        config,
                    });
                }
            }
        }

        for seat_bid in bid_response.seatbid {
            for bid in seat_bid.bid {
                let bid_type = internal.imp.iter()
                    .find(|imp| imp.id == bid.impid)
                    .map(|imp| get_bid_type_from_imp(imp))
                    .unwrap_or(BidType::Banner);

                // Extract video ext info
                let (deal_priority, bid_video) = if let Some(ext) = &bid.ext {
                    if let Ok(pm_ext) = serde_json::from_value::<PubmaticBidExt>(ext.clone()) {
                        let video = pm_ext.video.and_then(|v| v.duration).map(|dur| {
                            ExtBidPrebidVideo {
                                duration: dur,
                                primary_category: String::new(),
                            }
                        });
                        (pm_ext.deal_priority, video)
                    } else {
                        (0, None)
                    }
                } else {
                    (0, None)
                };

                let mut typed_bid = TypedBid::new(bid, bid_type);
                typed_bid.deal_priority = deal_priority;
                typed_bid.bid_video = bid_video;
                result.bids.push(typed_bid);
            }
        }

        Ok(result)
    }
}
