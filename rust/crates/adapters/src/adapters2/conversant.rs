use std::collections::HashMap;
use pbs_adapters::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct ConversantAdapter {
    pub endpoint: String,
}

impl ConversantAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
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
            break;
        }
    }
    BidType::Banner
}

impl Bidder for ConversantAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut req = request.clone();

        // Force USD currency
        if req.cur.as_ref().and_then(|c| c.first()).map(|s| s.as_str()) != Some("USD") {
            if req.cur.as_ref().map(|c| !c.is_empty()).unwrap_or(false) {
                req.cur = Some(vec!["USD".to_string()]);
            }
        }

        let mut site_id = String::new();

        for (i, imp) in req.imp.iter_mut().enumerate() {
            let bidder_ext = match imp.ext.as_ref().and_then(|e| e.get("bidder")) {
                Some(b) => b.clone(),
                None => return (vec![], vec![BidderError::BadInput(format!(
                    "Impression[{}] missing ext.bidder object", i
                ))]),
            };

            let cnvr_site_id = bidder_ext.get("site_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            if cnvr_site_id.is_empty() {
                return (vec![], vec![BidderError::BadInput(format!(
                    "Impression[{}] requires ext.bidder.site_id", i
                ))]);
            }

            if i == 0 {
                site_id = cnvr_site_id.to_string();
                if let Some(site) = req.site.as_mut() {
                    site.id = Some(site_id.clone());
                } else if let Some(app) = req.app.as_mut() {
                    app.id = Some(site_id.clone());
                }
            }

            // Apply conversant params to imp
            imp.displaymanager = Some("prebid-s2s".to_string());
            imp.displaymanagerver = Some("2.0.0".to_string());

            let bid_floor = bidder_ext.get("bidfloor").and_then(|v| v.as_f64()).unwrap_or(0.0);
            if imp.bidfloor.unwrap_or(0.0) <= 0.0 && bid_floor > 0.0 {
                imp.bidfloor = Some(bid_floor);
            }

            let tag_id = bidder_ext.get("tag_id").and_then(|v| v.as_str()).unwrap_or("");
            if !tag_id.is_empty() {
                imp.tagid = Some(tag_id.to_string());
            }

            // secure flag
            if let Some(secure_val) = bidder_ext.get("secure").and_then(|v| v.as_i64()) {
                if imp.secure.is_none() || imp.secure == Some(0) {
                    imp.secure = Some(secure_val as i32);
                }
            }

            // position
            let position = bidder_ext.get("position").and_then(|v| v.as_i64());

            if imp.banner.is_some() {
                if let Some(banner) = imp.banner.as_mut() {
                    if let Some(pos) = position {
                        banner.pos = Some(pos as i32);
                    }
                }
            } else if imp.video.is_some() {
                if let Some(video) = imp.video.as_mut() {
                    if let Some(pos) = position {
                        video.pos = Some(pos as i32);
                    }
                    // api
                    if let Some(api_arr) = bidder_ext.get("api").and_then(|v| v.as_array()) {
                        video.api = Some(api_arr.iter()
                            .filter_map(|v| v.as_i64().map(|n| n as i32))
                            .collect());
                    }
                    // protocols
                    if let Some(proto_arr) = bidder_ext.get("protocols").and_then(|v| v.as_array()) {
                        video.protocols = Some(proto_arr.iter()
                            .filter_map(|v| v.as_i64().map(|n| n as i32))
                            .collect());
                    }
                    // mimes
                    if let Some(mime_arr) = bidder_ext.get("mimes").and_then(|v| v.as_array()) {
                        let mimes: Vec<String> = mime_arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect();
                        if !mimes.is_empty() {
                            video.mimes = Some(mimes);
                        }
                    }
                    // maxduration
                    if let Some(max_dur) = bidder_ext.get("maxduration").and_then(|v| v.as_i64()) {
                        video.maxduration = Some(max_dur as i32);
                    }
                }
            }
        }

        let body = match serde_json::to_vec(&req) {
            Ok(b) => b,
            Err(_) => return (vec![], vec![BidderError::BadInput("Error in packaging request to JSON".to_string())]),
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
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
        _external: &RequestData,
        response: &ResponseData,
    ) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 {
            return Ok(BidderResponse::new());
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected status code: {}", response.status_code
            ))]);
        }

        let bid_response: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(format!("bad server response: {}. ", e))])?;

        if bid_response.seatbid.is_empty() {
            return Err(vec![BidderError::BadServerResponse("Empty bid request".to_string())]);
        }

        let bids = &bid_response.seatbid[0].bid;
        let mut result = BidderResponse::with_capacity(bids.len());
        for bid in bids {
            let bid_type = get_bid_type(&bid.impid, &internal.imp);
            result.bids.push(TypedBid::new(bid.clone(), bid_type));
        }

        Ok(result)
    }
}
