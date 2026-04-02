use std::collections::HashMap;
use pbs_adapters::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct RichaudienceAdapter {
    pub endpoint: String,
}

impl RichaudienceAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

fn get_media_type(imp_id: &str, imp: &openrtb::Imp) -> BidType {
    if imp.id == imp_id {
        if imp.video.is_some() {
            return BidType::Video;
        }
        return BidType::Banner;
    }
    BidType::Banner
}

impl Bidder for RichaudienceAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut requests = Vec::new();

        // Validate device: if device is present, IP or IPv6 must be set
        if let Some(device) = request.device.as_ref() {
            let ip = device.ip.as_deref().unwrap_or("");
            let ipv6 = device.ipv6.as_deref().unwrap_or("");
            if ip.is_empty() && ipv6.is_empty() {
                errs.push(BidderError::BadInput("request.Device.IP is required".to_string()));
                return (vec![], errs);
            }
        }

        // Determine if page URL is secure
        let is_url_secure = request.site.as_ref()
            .and_then(|s| s.page.as_deref())
            .map(|page| page.starts_with("https://"))
            .unwrap_or(false);

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
        headers.insert("X-Openrtb-Version".to_string(), "2.5".to_string());

        for imp in &request.imp {
            let mut imp = imp.clone();

            // Parse bidder ext
            let bidder_ext = imp.ext.as_ref()
                .and_then(|e| e.get("bidder"))
                .cloned();

            let pid = bidder_ext.as_ref()
                .and_then(|b| b.get("pid"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let test_mode = bidder_ext.as_ref()
                .and_then(|b| b.get("test"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let bid_floor_cur = bidder_ext.as_ref()
                .and_then(|b| b.get("bidFloorCur"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            if !pid.is_empty() {
                imp.tagid = Some(pid.clone());
            }

            if bid_floor_cur.is_empty() && imp.bidfloorcur.as_deref().unwrap_or("").is_empty() {
                imp.bidfloorcur = Some("USD".to_string());
            } else if !bid_floor_cur.is_empty() {
                imp.bidfloorcur = Some(bid_floor_cur);
            }

            let secure: i32 = if is_url_secure { 1 } else { 0 };
            imp.secure = Some(secure);

            // Validate banner
            if let Some(banner) = imp.banner.as_ref() {
                if banner.w.is_none() && banner.h.is_none() {
                    let has_formats = banner.format.as_ref().map(|f| !f.is_empty()).unwrap_or(false);
                    if !has_formats {
                        errs.push(BidderError::BadInput("request.Banner.Format is required".to_string()));
                        continue;
                    }
                }
            }

            // Validate video
            if let Some(video) = imp.video.as_ref() {
                let w_ok = video.w.map(|w| w > 0).unwrap_or(false);
                let h_ok = video.h.map(|h| h > 0).unwrap_or(false);
                if !w_ok || !h_ok {
                    errs.push(BidderError::BadInput("request.Video.Sizes is required".to_string()));
                    continue;
                }
            }

            let mut req = request.clone();

            // Set site/app keywords to tagid
            let tag_for_kw = imp.tagid.clone().unwrap_or_default();
            if let Some(site) = req.site.as_mut() {
                site.keywords = Some(format!("tagid={}", tag_for_kw));
            }
            if let Some(app) = req.app.as_mut() {
                app.keywords = Some(format!("tagid={}", tag_for_kw));
            }

            if test_mode {
                let device = req.device.get_or_insert_with(Default::default);
                device.ip = Some("11.222.33.44".to_string());
                req.test = Some(1);
            }

            req.imp = vec![imp];

            let body = match serde_json::to_vec(&req) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(BidderError::BadInput(e.to_string()));
                    continue;
                }
            };

            requests.push(RequestData {
                method: "POST".to_string(),
                uri: self.endpoint.clone(),
                body,
                headers: headers.clone(),
                imp_ids: get_imp_ids(&req.imp),
            });
        }

        (requests, errs)
    }

    fn make_bids(
        &self,
        _internal: &openrtb::BidRequest,
        external: &RequestData,
        response: &ResponseData,
    ) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 {
            return Ok(BidderResponse::new());
        }
        if let Err(e) = pbs_adapters::check_response_status(response.status_code) {
            return Err(vec![e]);
        }

        // Parse the request that was sent (to know the imp)
        let bid_req: openrtb::BidRequest = serde_json::from_slice(&external.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let bid_response: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(bid_req.imp.len());
        if let Some(cur) = bid_response.cur.as_deref() {
            if !cur.is_empty() {
                result.currency = cur.to_string();
            }
        }

        for req_imp in &bid_req.imp {
            for sb in &bid_response.seatbid {
                for bid in &sb.bid {
                    let bid_type = get_media_type(&bid.impid, req_imp);
                    let mut bid = bid.clone();
                    // Set w/h from video if video bid
                    if bid_type == BidType::Video {
                        if let Some(video) = req_imp.video.as_ref() {
                            if let Some(w) = video.w {
                                bid.w = Some(w);
                            }
                            if let Some(h) = video.h {
                                bid.h = Some(h);
                            }
                        }
                    }
                    result.bids.push(TypedBid::new(bid, bid_type));
                }
            }
        }

        Ok(result)
    }
}
