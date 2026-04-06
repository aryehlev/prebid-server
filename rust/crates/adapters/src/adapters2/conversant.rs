use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;

pub struct ConversantAdapter {
    pub endpoint: String,
}

impl ConversantAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

/// Conversant bidder extension from request imp
#[derive(Debug, Default, Deserialize)]
struct ExtImpConversant {
    #[serde(rename = "site_id", default)]
    site_id: String,
    #[serde(rename = "bidfloor", default)]
    bid_floor: f64,
    #[serde(rename = "tag_id", default)]
    tag_id: String,
    #[serde(rename = "position", default)]
    position: Option<i32>,
    #[serde(rename = "secure", default)]
    secure: Option<i32>,
    /// Video-specific fields
    #[serde(rename = "api", default)]
    api: Vec<i32>,
    #[serde(rename = "protocols", default)]
    protocols: Vec<i32>,
    #[serde(rename = "mimes", default)]
    mimes: Vec<String>,
    #[serde(rename = "maxduration", default)]
    max_duration: Option<i32>,
}

fn get_bid_type(imp_id: &str, imps: &[openrtb::Imp]) -> BidType {
    for imp in imps {
        if imp.id == imp_id {
            if imp.native.is_some() {
                return BidType::Native;
            } else if imp.audio.is_some() {
                return BidType::Audio;
            } else if imp.video.is_some() {
                return BidType::Video;
            }
        }
    }
    BidType::Banner
}

impl Bidder for ConversantAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut req = request.clone();

        // Backend needs USD
        if let Some(ref cur) = req.cur {
            if !cur.is_empty() && cur[0] != "USD" {
                req.cur = Some(vec!["USD".to_string()]);
            }
        }

        let mut site_id_set = false;

        for (i, imp) in req.imp.iter_mut().enumerate() {
            let bidder_val = match imp.ext.as_ref().and_then(|e| e.get("bidder")).cloned() {
                Some(v) => v,
                None => {
                    return (
                        vec![],
                        vec![BidderError::BadInput(format!(
                            "Impression[{}] missing ext object",
                            i
                        ))],
                    )
                }
            };

            let cnvr_ext: ExtImpConversant = match serde_json::from_value(bidder_val) {
                Ok(e) => e,
                Err(_) => {
                    return (
                        vec![],
                        vec![BidderError::BadInput(format!(
                            "Impression[{}] missing ext.bidder object",
                            i
                        ))],
                    )
                }
            };

            if cnvr_ext.site_id.is_empty() {
                return (
                    vec![],
                    vec![BidderError::BadInput(format!(
                        "Impression[{}] requires ext.bidder.site_id",
                        i
                    ))],
                );
            }

            // Set site_id on site or app from the first impression
            if !site_id_set {
                site_id_set = true;
                if let Some(site) = req.site.as_mut() {
                    site.id = Some(cnvr_ext.site_id.clone());
                } else if let Some(app) = req.app.as_mut() {
                    app.id = Some(cnvr_ext.site_id.clone());
                }
            }

            imp.displaymanager = Some("prebid-s2s".to_string());
            imp.displaymanagerver = Some("2.0.0".to_string());

            if imp.bidfloor.unwrap_or(0.0) <= 0.0 && cnvr_ext.bid_floor > 0.0 {
                imp.bidfloor = Some(cnvr_ext.bid_floor);
            }

            if !cnvr_ext.tag_id.is_empty() {
                imp.tagid = Some(cnvr_ext.tag_id.clone());
            }

            // Set secure flag if not already set globally
            if (imp.secure.is_none() || imp.secure == Some(0)) && cnvr_ext.secure.is_some() {
                imp.secure = cnvr_ext.secure;
            }

            // Apply position and video params
            if imp.banner.is_some() {
                if let Some(banner) = imp.banner.as_mut() {
                    banner.pos = cnvr_ext.position;
                }
            } else if imp.video.is_some() {
                if let Some(video) = imp.video.as_mut() {
                    video.pos = cnvr_ext.position;

                    if !cnvr_ext.api.is_empty() {
                        video.api = Some(cnvr_ext.api.clone());
                    }

                    if !cnvr_ext.protocols.is_empty() {
                        video.protocols = Some(cnvr_ext.protocols.clone());
                    }

                    if !cnvr_ext.mimes.is_empty() {
                        video.mimes = Some(cnvr_ext.mimes.clone());
                    }

                    if let Some(max_dur) = cnvr_ext.max_duration {
                        video.maxduration = Some(max_dur);
                    }
                }
            }
        }

        let body = match serde_json::to_vec(&req) {
            Ok(b) => b,
            Err(e) => {
                return (
                    vec![],
                    vec![BidderError::BadInput(format!(
                        "Error in packaging request to JSON: {}",
                        e
                    ))],
                )
            }
        };

        let mut headers = HashMap::new();
        headers.insert(
            "Content-Type".to_string(),
            "application/json;charset=utf-8".to_string(),
        );
        headers.insert("Accept".to_string(), "application/json".to_string());

        let imp_ids = get_imp_ids(&req.imp);
        (
            vec![RequestData {
                method: "POST".to_string(),
                uri: self.endpoint.clone(),
                body,
                headers,
                imp_ids,
            }],
            vec![],
        )
    }

    fn make_bids(
        &self,
        internal: &openrtb::BidRequest,
        _: &RequestData,
        response: &ResponseData,
    ) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 {
            return Ok(BidderResponse::new());
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected status code: {}",
                response.status_code
            ))]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body).map_err(|e| {
            vec![BidderError::BadServerResponse(format!(
                "bad server response: {}. ",
                e
            ))]
        })?;

        if bid_resp.seatbid.is_empty() {
            return Err(vec![BidderError::BadServerResponse(
                "Empty bid request".to_string(),
            )]);
        }

        let bids = &bid_resp.seatbid[0].bid;
        let mut result = BidderResponse::with_capacity(bids.len());
        for bid in bids {
            let bid_type = get_bid_type(&bid.impid, &internal.imp);
            result.bids.push(TypedBid::new(bid.clone(), bid_type));
        }
        Ok(result)
    }
}
