use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;

pub struct StroeerCoreAdapter { pub endpoint: String }
impl StroeerCoreAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

#[derive(Debug, Deserialize)]
struct StroeerBidResponse {
    #[serde(default)]
    bids: Vec<StroeerBid>,
}

#[derive(Debug, Deserialize)]
struct StroeerBid {
    id: String,
    #[serde(rename = "bidId")]
    bid_id: String,
    cpm: f64,
    width: i32,
    height: i32,
    ad: String,
    #[serde(default)]
    crid: String,
    mtype: String,
    #[serde(default)]
    adomain: Vec<String>,
    ext: Option<serde_json::Value>,
    dsa: Option<serde_json::Value>,
}

fn get_bid_ext(bid: &StroeerBid) -> Option<serde_json::Value> {
    match (&bid.dsa, &bid.ext) {
        (None, ext) => ext.clone(),
        (Some(dsa), ext) => {
            let mut map = serde_json::Map::new();
            if let Some(ext_val) = ext {
                if let Some(obj) = ext_val.as_object() {
                    for (k, v) in obj {
                        map.insert(k.clone(), v.clone());
                    }
                }
            }
            map.insert("dsa".to_string(), dsa.clone());
            Some(serde_json::Value::Object(map))
        }
    }
}

impl Bidder for StroeerCoreAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut req_copy = request.clone();
        let mut errs = Vec::new();

        for imp in &mut req_copy.imp {
            if let Some(sid) = imp.ext.as_ref()
                .and_then(|e| e.get("bidder"))
                .and_then(|b| b.get("sid"))
                .and_then(|v| v.as_str())
            {
                imp.tagid = Some(sid.to_string());
            }
        }

        let body = match serde_json::to_vec(&req_copy) {
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
        }], errs)
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected http status code: {}.", response.status_code
            ))]);
        }
        let stroeer_resp: StroeerBidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(stroeer_resp.bids.len());
        result.currency = "EUR".to_string();
        let mut errs = Vec::new();

        for bid in stroeer_resp.bids {
            let (markup_type, bid_type) = match bid.mtype.as_str() {
                "banner" => (1i32, BidType::Banner),
                "video" => (2i32, BidType::Video),
                _ => {
                    errs.push(BidderError::BadServerResponse(format!(
                        "Bid media type error: unable to determine media type for bid with id \"{}\"",
                        bid.bid_id
                    )));
                    continue;
                }
            };

            let ext_json = get_bid_ext(&bid);

            let openrtb_bid = openrtb::Bid {
                id: bid.id.clone(),
                impid: bid.bid_id.clone(),
                w: Some(bid.width),
                h: Some(bid.height),
                price: bid.cpm,
                adm: Some(bid.ad.clone()),
                crid: Some(bid.crid.clone()),
                mtype: Some(markup_type),
                adomain: if bid.adomain.is_empty() { None } else { Some(bid.adomain.clone()) },
                ext: ext_json,
                ..Default::default()
            };

            result.bids.push(TypedBid::new(openrtb_bid, bid_type));
        }

        if !errs.is_empty() { return Err(errs); }
        Ok(result)
    }
}
