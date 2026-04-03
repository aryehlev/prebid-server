use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use openrtb_ext::BidType;
use serde::{Deserialize, Serialize};

pub struct MobkoiAdapter { pub endpoint: String }
impl MobkoiAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct UserExt {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    consent: String,
}

impl Bidder for MobkoiAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::BadInput("no impressions".to_string())]);
        }

        let bidder = request.imp[0].ext.as_ref().and_then(|e| e.get("bidder"));
        let placement_id = bidder.and_then(|b| b.get("placementId")).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let tag_id = if !placement_id.is_empty() {
            placement_id
        } else {
            request.imp[0].tagid.clone().unwrap_or_default()
        };
        if tag_id.is_empty() {
            return (vec![], vec![BidderError::BadInput(
                "invalid because it comes with neither request.imp[0].tagId nor req.imp[0].ext.Bidder.placementId".to_string()
            )]);
        }

        let mut req_copy = request.clone();
        req_copy.imp[0].tagid = Some(tag_id);

        // If user has consent in ext, set it on user.ext
        if let Some(user) = &request.user {
            if let Some(ext) = &user.ext {
                let user_ext: UserExt = serde_json::from_value(ext.clone()).unwrap_or_default();
                if !user_ext.consent.is_empty() {
                    let mut user_copy = user.clone();
                    user_copy.ext = serde_json::to_value(UserExt { consent: user_ext.consent }).ok();
                    req_copy.user = Some(user_copy);
                }
            }
        }

        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
        let imp_ids = req_copy.imp.iter().map(|i| i.id.clone()).collect();
        (vec![RequestData { method: "POST".to_string(), uri: self.endpoint.clone(), body, headers, imp_ids }], vec![])
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(1);
        if let Some(cur) = &bid_resp.cur { result.currency = cur.clone(); }
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                result.bids.push(TypedBid::new(bid, BidType::Banner));
            }
        }
        Ok(result)
    }
}
