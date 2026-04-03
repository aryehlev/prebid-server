use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;
use serde_json::Value;

pub struct AkceloAdapter { pub endpoint: String }
impl AkceloAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

#[derive(Deserialize)]
struct ExtImpBidder { bidder: Value }

#[derive(Deserialize)]
struct ExtImpAkcelo {
    #[serde(rename = "siteId", default)]
    site_id: Value,
}

fn mtype_to_bid_type(mtype: Option<i32>) -> BidType {
    match mtype {
        Some(2) => BidType::Video,
        Some(4) => BidType::Native,
        _ => BidType::Banner,
    }
}

impl Bidder for AkceloAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::BadInput("No valid Imp".to_string())]);
        }
        // Extract siteId from first imp to configure publisher parent account
        let site_id = request.imp[0].ext.as_ref()
            .and_then(|e| serde_json::from_value::<ExtImpBidder>(e.clone()).ok())
            .and_then(|be| serde_json::from_value::<ExtImpAkcelo>(be.bidder).ok())
            .map(|ext| ext.site_id.to_string().trim_matches('"').to_string())
            .unwrap_or_default();
        if site_id.is_empty() {
            return (vec![], vec![BidderError::BadInput("Cannot find valid siteId".to_string())]);
        }
        // Build request with publisher parent account
        let mut req = request.clone();
        if let Some(site) = &mut req.site {
            let publisher = site.publisher.get_or_insert_with(Default::default);
            let ext_val = serde_json::json!({
                "prebid": { "parentAccount": site_id }
            });
            publisher.ext = Some(ext_val);
        } else {
            req.site = Some(openrtb::Site {
                publisher: Some(openrtb::Publisher {
                    ext: Some(serde_json::json!({ "prebid": { "parentAccount": site_id } })),
                    ..Default::default()
                }),
                ..Default::default()
            });
        }
        let body = match serde_json::to_vec(&req) {
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
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(5);
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bt = mtype_to_bid_type(bid.mtype);
                result.bids.push(TypedBid::new(bid, bt));
            }
        }
        Ok(result)
    }
}
