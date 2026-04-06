use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;

pub struct MabidderAdapter { pub endpoint: String }
impl MabidderAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

#[derive(Deserialize)]
struct ServerResponse {
    #[serde(rename = "Responses", default)]
    responses: Vec<BidResponseItem>,
}

#[derive(Deserialize)]
struct BidResponseItem {
    #[serde(rename = "requestId", default)]
    request_id: String,
    #[serde(rename = "currency", default)]
    currency: String,
    #[serde(rename = "width", default)]
    width: i64,
    #[serde(rename = "height", default)]
    height: i64,
    #[serde(rename = "creativeId", default)]
    creative_id: String,
    #[serde(rename = "dealId", default)]
    deal_id: String,
    #[serde(rename = "ttl", default)]
    _ttl: i32,
    #[serde(rename = "ad", default)]
    ad: String,
    #[serde(rename = "mediaType", default)]
    media_type: String,
    #[serde(rename = "meta", default)]
    meta: BidMeta,
    #[serde(rename = "cpm", default)]
    cpm: f64,
}

#[derive(Deserialize, Default)]
struct BidMeta {
    #[serde(rename = "advertiserDomains", default)]
    advertiser_domains: Vec<String>,
}

fn get_bid_type_from_media_type(media_type: &str) -> BidType {
    match media_type {
        "video" => BidType::Video,
        "native" => BidType::Native,
        "audio" => BidType::Audio,
        _ => BidType::Banner,
    }
}

impl Bidder for MabidderAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let body = match serde_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
        (vec![RequestData { method: "POST".to_string(), uri: self.endpoint.clone(), body, headers, imp_ids: get_imp_ids(&request.imp) }], vec![])
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }

        let server_resp: ServerResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(server_resp.responses.len());

        for item in server_resp.responses {
            let bid_type = get_bid_type_from_media_type(&item.media_type);

            let mut bid = openrtb::Bid::default();
            bid.id = item.request_id.clone();
            bid.impid = item.request_id.clone();
            bid.price = item.cpm;
            bid.adm = if item.ad.is_empty() { None } else { Some(item.ad) };
            bid.w = if item.width != 0 { Some(item.width as i32) } else { None };
            bid.h = if item.height != 0 { Some(item.height as i32) } else { None };
            bid.crid = if item.creative_id.is_empty() { None } else { Some(item.creative_id) };
            bid.dealid = if item.deal_id.is_empty() { None } else { Some(item.deal_id) };
            bid.adomain = if item.meta.advertiser_domains.is_empty() { None } else { Some(item.meta.advertiser_domains) };

            if !item.currency.is_empty() {
                result.currency = item.currency;
            }

            result.bids.push(TypedBid::new(bid, bid_type));
        }

        Ok(result)
    }
}
