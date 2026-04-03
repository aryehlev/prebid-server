use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids, check_response_status};
use openrtb::BidResponse;
use openrtb_ext::BidType;

pub struct MarsmediaAdapter { pub endpoint: String }
impl MarsmediaAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

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

impl Bidder for MarsmediaAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::BadInput("No impression in the bid request".to_string())]);
        }

        // Get zoneId from first imp ext.bidder
        let zone_id = request.imp.first()
            .and_then(|imp| imp.ext.as_ref())
            .and_then(|e| e.get("bidder"))
            .and_then(|b| b.get("zoneId"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        if zone_id.is_empty() {
            return (vec![], vec![BidderError::BadInput("zoneId is empty".to_string())]);
        }

        // Validate imps: banner must have format or w/h; video is ok
        let mut req_copy = request.clone();
        let mut valid_imp_exists = false;
        for imp in &mut req_copy.imp {
            if let Some(banner) = &mut imp.banner {
                let has_format = banner.format.as_ref().map(|f| !f.is_empty()).unwrap_or(false);
                if has_format {
                    let first = banner.format.as_ref().unwrap()[0].clone();
                    banner.w = first.w;
                    banner.h = first.h;
                    valid_imp_exists = true;
                } else if banner.w.is_some() && banner.h.is_some() {
                    valid_imp_exists = true;
                } else {
                    return (vec![], vec![BidderError::BadInput("No valid banner format in the bid request".to_string())]);
                }
            } else if imp.video.is_some() {
                valid_imp_exists = true;
            }
        }
        if !valid_imp_exists {
            return (vec![], vec![BidderError::BadInput("No valid impression in the bid request".to_string())]);
        }

        req_copy.at = Some(1); // first price auction

        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let uri = format!("{}&zone={}", self.endpoint, zone_id);
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
        headers.insert("x-openrtb-version".to_string(), "2.5".to_string());

        if let Some(device) = &request.device {
            if let Some(ua) = &device.ua {
                if !ua.is_empty() {
                    headers.insert("User-Agent".to_string(), ua.clone());
                }
            }
            if let Some(ip) = &device.ip {
                if !ip.is_empty() {
                    headers.insert("X-Forwarded-For".to_string(), ip.clone());
                }
            }
            if let Some(lang) = &device.language {
                if !lang.is_empty() {
                    headers.insert("Accept-Language".to_string(), lang.clone());
                }
            }
            if let Some(dnt) = device.dnt {
                headers.insert("DNT".to_string(), dnt.to_string());
            }
        }

        let imp_ids = get_imp_ids(&req_copy.imp);
        (vec![RequestData {
            method: "POST".to_string(),
            uri,
            body,
            headers,
            imp_ids,
        }], vec![])
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        if bid_resp.seatbid.is_empty() {
            return Ok(BidderResponse::new());
        }
        let sb = &bid_resp.seatbid[0];
        let mut result = BidderResponse::with_capacity(sb.bid.len());
        for bid in bid_resp.seatbid.into_iter().flat_map(|sb| sb.bid) {
            let bid_type = get_media_type_for_imp(&bid.impid, &internal.imp);
            result.bids.push(TypedBid::new(bid, bid_type));
        }
        Ok(result)
    }
}
