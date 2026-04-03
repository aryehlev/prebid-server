use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::{Deserialize, Serialize};

pub struct TelariaAdapter { pub endpoint: String }
impl TelariaAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

#[derive(Debug, Deserialize)]
struct ExtImpTelaria {
    #[serde(rename = "seatCode", default)]
    seat_code: String,
    #[serde(rename = "adCode", default)]
    ad_code: String,
    #[serde(rename = "extra")]
    extra: Option<serde_json::Value>,
}

#[derive(Serialize)]
struct ImpressionExtOut {
    #[serde(rename = "originalTagid")]
    original_tagid: String,
    #[serde(rename = "originalPublisherid")]
    original_publisherid: String,
}

#[derive(Serialize)]
struct TelariaBidExt {
    #[serde(skip_serializing_if = "Option::is_none")]
    extra: Option<serde_json::Value>,
}

impl Bidder for TelariaAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::BadInput("Telaria: Missing Imp Object".to_string())]);
        }

        // Only supports video, no banner
        let has_video = request.imp.iter().any(|i| i.video.is_some());
        let has_banner = request.imp.iter().any(|i| i.banner.is_some());
        if has_banner {
            return (vec![], vec![BidderError::BadInput("Telaria: Banner not supported".to_string())]);
        }
        if !has_video {
            return (vec![], vec![BidderError::BadInput("Telaria: Only Supports Video".to_string())]);
        }

        let mut req_copy = request.clone();

        // Get telaria ext from first imp
        let bidder_val = match req_copy.imp[0].ext.as_ref().and_then(|e| e.get("bidder")) {
            Some(v) => v.clone(),
            None => return (vec![], vec![BidderError::BadInput("Telaria: ext.bidder not provided".to_string())]),
        };
        let telaria_ext: ExtImpTelaria = match serde_json::from_value(bidder_val) {
            Ok(e) => e,
            Err(_) => return (vec![], vec![BidderError::BadInput("Telaria: invalid bidder ext".to_string())]),
        };
        if telaria_ext.seat_code.is_empty() {
            return (vec![], vec![BidderError::BadInput("Telaria: Seat Code required".to_string())]);
        }

        // Get original publisher ID before modifying
        let original_publisher_id = req_copy.site.as_ref()
            .and_then(|s| s.publisher.as_ref())
            .and_then(|p| p.id.clone())
            .or_else(|| req_copy.app.as_ref()
                .and_then(|a| a.publisher.as_ref())
                .and_then(|p| p.id.clone()))
            .unwrap_or_default();

        let original_tag_id = req_copy.imp[0].tagid.clone().unwrap_or_default();

        // Move original tag/publisher into imp.ext
        let ext_out = ImpressionExtOut {
            original_tagid: original_tag_id,
            original_publisherid: original_publisher_id,
        };
        req_copy.imp[0].ext = serde_json::to_value(&ext_out).ok();

        // Swap tagID with adCode
        req_copy.imp[0].tagid = Some(telaria_ext.ad_code.clone());

        // Add extra from imp to top-level ext
        if let Some(extra) = &telaria_ext.extra {
            req_copy.ext = serde_json::to_value(TelariaBidExt { extra: Some(extra.clone()) }).ok();
        }

        // Set publisher.ID to seatCode
        let seat_code = telaria_ext.seat_code.clone();
        if let Some(site) = &mut req_copy.site {
            let mut publisher = site.publisher.clone().unwrap_or_default();
            publisher.id = Some(seat_code.clone());
            site.publisher = Some(publisher);
            req_copy.app = None;
        } else if let Some(app) = &mut req_copy.app {
            let mut publisher = app.publisher.clone().unwrap_or_default();
            publisher.id = Some(seat_code.clone());
            app.publisher = Some(publisher);
        }

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
        headers.insert("X-Openrtb-Version".to_string(), "2.5".to_string());

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
                headers.insert("Dnt".to_string(), dnt.to_string());
            }
        }

        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        (vec![RequestData {
            method: "POST".to_string(),
            uri: self.endpoint.clone(),
            body,
            headers,
            imp_ids: get_imp_ids(&request.imp),
        }], vec![])
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 {
            return Err(vec![BidderError::BadInput("Telaria: Invalid Bid Request received by the server".to_string())]);
        }
        if response.status_code == 400 || response.status_code == 503 {
            return Err(vec![BidderError::BadInput(format!(
                "Telaria: Unexpected status code: [ {} ] ", response.status_code
            ))]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadInput(format!(
                "Telaria: Something went wrong, please contact your Account Manager. Status Code: [ {} ] ", response.status_code
            ))]);
        }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|_| vec![BidderError::BadServerResponse("Telaria: Bad Server Response".to_string())])?;

        if bid_resp.seatbid.is_empty() {
            return Ok(BidderResponse::new());
        }

        let sb = &bid_resp.seatbid[0];
        let mut result = BidderResponse::with_capacity(sb.bid.len());

        for (i, mut bid) in sb.bid.clone().into_iter().enumerate() {
            if i >= internal.imp.len() { break; }
            bid.impid = internal.imp[i].id.clone();
            result.bids.push(TypedBid::new(bid, BidType::Video));
        }
        Ok(result)
    }
}
