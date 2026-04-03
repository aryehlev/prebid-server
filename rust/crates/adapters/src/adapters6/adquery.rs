use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use openrtb_ext::BidType;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub struct AdqueryAdapter { pub endpoint: String }
impl AdqueryAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

#[derive(Deserialize)]
struct ExtImpBidder { bidder: Value }

#[derive(Deserialize)]
struct ImpExtAdQuery {
    #[serde(rename = "placementId", default)]
    placement_id: String,
    #[serde(rename = "type", default)]
    bid_type: String,
}

#[derive(Serialize)]
struct BidderRequest {
    v: String,
    #[serde(rename = "placementCode")]
    placement_code: String,
    #[serde(rename = "auctionId")]
    auction_id: String,
    #[serde(rename = "bidType")]
    bid_type: String,
    #[serde(rename = "adUnitCode")]
    ad_unit_code: String,
    #[serde(rename = "bidQid")]
    bid_qid: String,
    #[serde(rename = "bidId")]
    bid_id: String,
    bidder: String,
    #[serde(rename = "bidderRequestId")]
    bidder_request_id: String,
    #[serde(rename = "bidRequestsCount")]
    bid_requests_count: i32,
    #[serde(rename = "bidderRequestsCount")]
    bidder_requests_count: i32,
    sizes: String,
    #[serde(rename = "bidIp", default, skip_serializing_if = "String::is_empty")]
    bid_ip: String,
    #[serde(rename = "bidIpv6", default, skip_serializing_if = "String::is_empty")]
    bid_ipv6: String,
    #[serde(rename = "bidUa", default, skip_serializing_if = "String::is_empty")]
    bid_ua: String,
    #[serde(rename = "bidPageUrl", default, skip_serializing_if = "String::is_empty")]
    bid_page_url: String,
}

#[derive(Deserialize)]
struct AdqueryResponse {
    data: Option<ResponseData2>,
}

#[derive(Deserialize)]
struct ResponseData2 {
    #[serde(rename = "reqId", default)]
    req_id: String,
    #[serde(rename = "cpm", default)]
    cpm: String,
    #[serde(rename = "currency", default)]
    currency: String,
    #[serde(rename = "tag", default)]
    tag: String,
    #[serde(rename = "adQLib", default)]
    ad_q_lib: String,
    #[serde(rename = "aDomains", default)]
    a_domains: Vec<String>,
    #[serde(rename = "crId", default)]
    cr_id: u64,
    #[serde(rename = "mediaType")]
    media_type: MediaType,
}

#[derive(Deserialize, Default)]
struct MediaType {
    #[serde(default)]
    name: String,
    #[serde(default)]
    width: String,
    #[serde(default)]
    height: String,
}

fn get_imp_sizes(imp: &openrtb::Imp) -> String {
    if let Some(banner) = &imp.banner {
        if let Some(formats) = &banner.format {
            if !formats.is_empty() {
                let parts: Vec<String> = formats.iter()
                    .map(|f| format!("{}x{}", f.w.unwrap_or(0), f.h.unwrap_or(0)))
                    .collect();
                return parts.join(",");
            }
        }
        if let (Some(w), Some(h)) = (banner.w, banner.h) {
            return format!("{}x{}", w, h);
        }
    }
    String::new()
}

impl Bidder for AdqueryAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
        headers.insert("X-Openrtb-Version".to_string(), "2.5".to_string());
        if let Some(device) = &request.device {
            if let Some(ip) = &device.ip { if !ip.is_empty() { headers.insert("X-Forwarded-For".to_string(), ip.clone()); } }
        }
        let mut requests = Vec::new();
        let mut errs = Vec::new();
        for imp in &request.imp {
            let bidder_ext: ExtImpBidder = match imp.ext.as_ref()
                .and_then(|e| serde_json::from_value(e.clone()).ok()) {
                Some(v) => v,
                None => { errs.push(BidderError::BadInput(format!("failed to parse ext for imp {}", imp.id))); continue; }
            };
            let ext: ImpExtAdQuery = match serde_json::from_value(bidder_ext.bidder) {
                Ok(v) => v,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };
            let user_id = request.user.as_ref().map(|u| u.id.clone()).unwrap_or_default();
            let bid_req = BidderRequest {
                v: "server".to_string(),
                placement_code: ext.placement_id,
                auction_id: String::new(),
                bid_type: ext.bid_type,
                ad_unit_code: imp.tagid.clone().unwrap_or_default(),
                bid_qid: user_id.unwrap_or_default(),
                bid_id: format!("{}{}", request.id, imp.id),
                bidder: "adquery".to_string(),
                bidder_request_id: request.id.clone(),
                bid_requests_count: 1,
                bidder_requests_count: 1,
                sizes: get_imp_sizes(imp),
                bid_ip: request.device.as_ref().and_then(|d| d.ip.clone()).unwrap_or_default(),
                bid_ipv6: request.device.as_ref().and_then(|d| d.ipv6.clone()).unwrap_or_default(),
                bid_ua: request.device.as_ref().and_then(|d| d.ua.clone()).unwrap_or_default(),
                bid_page_url: request.site.as_ref().and_then(|s| s.page.clone()).unwrap_or_default(),
            };
            let body = match serde_json::to_vec(&bid_req) {
                Ok(b) => b,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };
            requests.push(RequestData { method: "POST".to_string(), uri: self.endpoint.clone(), body, headers: headers.clone(), imp_ids: vec![imp.id.clone()] });
        }
        (requests, errs)
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let resp: AdqueryResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let data = match resp.data { Some(d) => d, None => return Ok(BidderResponse::new()) };
        if data.media_type.name != "banner" {
            return Err(vec![BidderError::BadServerResponse(format!("unsupported MediaType: {}", data.media_type.name))]);
        }
        let price: f64 = data.cpm.parse().map_err(|e: std::num::ParseFloatError| vec![BidderError::BadServerResponse(e.to_string())])?;
        let width: i32 = data.media_type.width.parse().unwrap_or(0);
        let height: i32 = data.media_type.height.parse().unwrap_or(0);
        // imp_id is req_id minus the request ID prefix
        let imp_id = data.req_id.strip_prefix(&internal.id).unwrap_or(&data.req_id).to_string();
        let bid = openrtb::Bid {
            id: data.req_id.clone(),
            impid: imp_id,
            price,
            adm: Some(format!("<script src=\"{}\"></script>{}", data.ad_q_lib, data.tag)),
            adomain: Some(data.a_domains),
            crid: Some(format!("{}", data.cr_id)),
            w: Some(width),
            h: Some(height),
            ..Default::default()
        };
        let mut result = BidderResponse::with_capacity(1);
        result.currency = if data.currency.is_empty() { "PLN".to_string() } else { data.currency };
        result.bids.push(TypedBid::new(bid, BidType::Banner));
        Ok(result)
    }
}
