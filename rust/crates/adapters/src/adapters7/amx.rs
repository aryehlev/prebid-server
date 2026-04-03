use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;
use openrtb_ext::ExtBidPrebidMeta;

pub struct AmxAdapter {
    pub endpoint: String,
}

impl AmxAdapter {
    pub fn new(endpoint: String) -> Self {
        // Append version param to endpoint, like Go does: ?v=pbs1.2
        let endpoint = if endpoint.contains('?') {
            format!("{}&v=pbs1.2", endpoint)
        } else {
            format!("{}?v=pbs1.2", endpoint)
        };
        Self { endpoint }
    }
}

#[derive(Deserialize)]
struct ExtImpAMX {
    #[serde(rename = "tagId", default)]
    tag_id: String,
    #[serde(rename = "adUnitId", default)]
    ad_unit_id: String,
}

#[derive(Deserialize)]
struct AmxBidExt {
    #[serde(rename = "startdelay")]
    start_delay: Option<i32>,
    #[serde(rename = "ct")]
    creative_type: Option<i32>,
    #[serde(rename = "ds")]
    demand_source: Option<String>,
    #[serde(rename = "bc")]
    bidder_code: Option<String>,
}

fn get_media_type(bid_ext: &AmxBidExt) -> BidType {
    if bid_ext.start_delay.is_some() {
        return BidType::Video;
    }
    if bid_ext.creative_type == Some(10) {
        return BidType::Native;
    }
    BidType::Banner
}

impl Bidder for AmxAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut req = request.clone();

        let mut publisher_id = String::new();

        for imp in &mut req.imp {
            if let Some(ext) = &imp.ext {
                #[derive(Deserialize)]
                struct AmxImpExt { bidder: Option<ExtImpAMX> }
                if let Ok(params) = serde_json::from_value::<AmxImpExt>(ext.clone()) {
                    if let Some(bidder) = params.bidder {
                        if !bidder.tag_id.is_empty() {
                            publisher_id = bidder.tag_id;
                        }
                        if !bidder.ad_unit_id.is_empty() {
                            imp.tagid = Some(bidder.ad_unit_id);
                        }
                    }
                }
            }
        }

        if !publisher_id.is_empty() {
            if let Some(site) = &mut req.site {
                let mut pub_obj = site.publisher.clone().unwrap_or_default();
                pub_obj.id = Some(publisher_id.clone());
                site.publisher = Some(pub_obj);
            }
            if let Some(app) = &mut req.app {
                let mut pub_obj = app.publisher.clone().unwrap_or_default();
                pub_obj.id = Some(publisher_id.clone());
                app.publisher = Some(pub_obj);
            }
        }

        let body = match serde_json::to_vec(&req) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        (vec![RequestData {
            method: "POST".to_string(),
            uri: self.endpoint.clone(),
            body,
            headers,
            imp_ids: get_imp_ids(&request.imp),
        }], vec![])
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code == 400 {
            // Read error from response headers - just use generic message
            return Err(vec![BidderError::BadInput(format!("Invalid Request: 400. Error Code: "))]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!("Unexpected response: {}. Error Code: ", response.status_code))]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(5);
        let mut errs = Vec::new();

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_ext = bid.ext.as_ref()
                    .and_then(|e| serde_json::from_value::<AmxBidExt>(e.clone()).ok())
                    .unwrap_or(AmxBidExt { start_delay: None, creative_type: None, demand_source: None, bidder_code: None });

                let bid_type = get_media_type(&bid_ext);
                let demand_source = bid_ext.demand_source.clone().unwrap_or_default();
                let adomain = bid.adomain.clone().unwrap_or_default();

                let mut typed_bid = TypedBid::new(bid, bid_type);
                typed_bid.bid_meta = Some(ExtBidPrebidMeta {
                    advertiser_domains: Some(adomain),
                    demand_source: Some(demand_source),
                    ..Default::default()
                });

                result.bids.push(typed_bid);
            }
        }

        if !errs.is_empty() && result.bids.is_empty() {
            return Err(errs);
        }

        Ok(result)
    }
}
