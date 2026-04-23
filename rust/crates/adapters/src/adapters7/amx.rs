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
    /// bidder_code corresponds to Go's bc field; used to set seat on the bid
    /// (TypedBid does not currently have a seat field in the Rust port)
    #[serde(rename = "bc")]
    #[allow(dead_code)]
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
            let nbr = response.headers.get("x-nbr").map(|s| s.as_str()).unwrap_or("");
            return Err(vec![BidderError::BadInput(format!("Invalid Request: 400. Error Code: {}", nbr))]);
        }
        if response.status_code != 200 {
            let nbr = response.headers.get("x-nbr").map(|s| s.as_str()).unwrap_or("");
            return Err(vec![BidderError::BadServerResponse(format!("Unexpected response: {}. Error Code: {}", response.status_code, nbr))]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(5);
        let mut errs = Vec::new();

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                // Parse bid ext - if ext is non-empty but fails to parse, skip the bid
                let bid_ext = if let Some(ext) = &bid.ext {
                    match serde_json::from_value::<AmxBidExt>(ext.clone()) {
                        Ok(v) => v,
                        Err(e) => {
                            errs.push(BidderError::BadServerResponse(e.to_string()));
                            continue;
                        }
                    }
                } else {
                    AmxBidExt { start_delay: None, creative_type: None, demand_source: None, bidder_code: None }
                };

                let bid_type = get_media_type(&bid_ext);
                let demand_source = bid_ext.demand_source.clone().unwrap_or_default();
                let adomain = bid.adomain.clone().unwrap_or_default();

                let mut typed_bid = TypedBid::new(bid, bid_type);
                typed_bid.bid_meta = Some(ExtBidPrebidMeta {
                    advertiser_domains: Some(adomain),
                    demand_source: Some(demand_source),
                    ..Default::default()
                });
                // Note: Go sets b.Seat = bidder_code, but TypedBid has no seat field in Rust

                result.bids.push(typed_bid);
            }
        }

        if !errs.is_empty() && result.bids.is_empty() {
            return Err(errs);
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_req() -> openrtb::BidRequest {
        openrtb::BidRequest {
            id: "r".to_string(),
            imp: vec![openrtb::Imp {
                id: "i1".to_string(),
                banner: Some(openrtb::Banner { w: Some(300), h: Some(250), ..Default::default() }),
                ext: Some(serde_json::json!({"bidder": {"tagId": "pub1", "adUnitId": "unit1"}})),
                ..Default::default()
            }],
            site: Some(openrtb::Site::default()),
            ..Default::default()
        }
    }

    #[test]
    fn test_make_requests_basic() {
        let a = AmxAdapter::new("https://prebid.a-mo.net/a/c".to_string());
        let (reqs, errs) = a.make_requests(&make_req(), &ExtraRequestInfo::default());
        assert!(errs.is_empty());
        assert_eq!(reqs.len(), 1);
        assert!(reqs[0].uri.contains("v=pbs1.2"));
        assert_eq!(reqs[0].headers.get("Content-Type").unwrap(), "application/json;charset=utf-8");
        let body: serde_json::Value = serde_json::from_slice(&reqs[0].body).unwrap();
        assert_eq!(body["imp"][0]["tagid"], "unit1");
        assert_eq!(body["site"]["publisher"]["id"], "pub1");
    }

    #[test]
    fn test_make_bids_basic() {
        let a = AmxAdapter::new("https://x".to_string());
        let body = br#"{"id":"r","seatbid":[{"bid":[{"id":"b1","impid":"i1","price":1.0}]}]}"#;
        let resp = ResponseData::new(200, body.to_vec());
        let result = a.make_bids(&make_req(), &RequestData::default(), &resp).unwrap();
        assert_eq!(result.bids.len(), 1);
        assert_eq!(result.bids[0].bid_type, BidType::Banner);
    }
}
