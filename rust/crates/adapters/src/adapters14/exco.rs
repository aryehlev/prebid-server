use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct ExcoAdapter { pub endpoint: String }
impl ExcoAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

fn get_bid_type_from_mtype(mtype: i32) -> Result<BidType, BidderError> {
    match mtype {
        1 => Ok(BidType::Banner),
        2 => Ok(BidType::Video),
        _ => Err(BidderError::BadServerResponse(
            format!("unrecognized bid_ad_type in response from exco: {}", mtype)
        )),
    }
}

impl Bidder for ExcoAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut req = request.clone();
        let mut publisher_id = String::new();

        // Extract publisher_id and tag_id from each imp ext
        for (i, imp) in req.imp.iter_mut().enumerate() {
            let bidder = imp.ext.as_ref()
                .and_then(|e| e.get("bidder"))
                .cloned()
                .unwrap_or(serde_json::Value::Null);

            if bidder.is_null() {
                return (vec![], vec![BidderError::BadInput(
                    format!("Invalid imp.ext for impression index {}", i)
                )]);
            }

            let pid = bidder.get("publisherId").and_then(|v| v.as_str()).unwrap_or("");
            let tag_id = bidder.get("tagId").and_then(|v| v.as_str()).unwrap_or("").to_string();

            publisher_id = pid.to_string();
            imp.tagid = Some(tag_id);
        }

        // Set publisher ID on site or app
        if req.site.is_some() {
            let site = req.site.as_mut().unwrap();
            if site.publisher.is_none() {
                site.publisher = Some(openrtb::Publisher::default());
            }
            if let Some(pub_) = site.publisher.as_mut() {
                pub_.id = Some(publisher_id.clone());
            }
        }
        if req.app.is_some() {
            let app = req.app.as_mut().unwrap();
            if app.publisher.is_none() {
                app.publisher = Some(openrtb::Publisher::default());
            }
            if let Some(pub_) = app.publisher.as_mut() {
                pub_.id = Some(publisher_id.clone());
            }
        }

        let body = match serde_json::to_vec(&req) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());

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
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::new();
        let mut errors = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let mtype = bid.mtype.unwrap_or(0);
                match get_bid_type_from_mtype(mtype) {
                    Ok(bid_type) => result.bids.push(TypedBid::new(bid, bid_type)),
                    Err(e) => errors.push(e),
                }
            }
        }

        if !errors.is_empty() {
            Err(errors)
        } else {
            Ok(result)
        }
    }
}
