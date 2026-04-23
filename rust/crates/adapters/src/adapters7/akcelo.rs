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

fn get_bid_type(bid: &openrtb::Bid) -> Result<BidType, BidderError> {
    if let Some(mtype) = bid.mtype {
        return match mtype {
            1 => Ok(BidType::Banner),
            2 => Ok(BidType::Video),
            4 => Ok(BidType::Native),
            _ => Err(BidderError::BadServerResponse(format!("unable to get media type {}", mtype))),
        };
    }
    if let Some(ext) = &bid.ext {
        if let Some(prebid) = ext.get("prebid") {
            if let Some(type_val) = prebid.get("type") {
                if let Some(type_str) = type_val.as_str() {
                    return match type_str {
                        "banner" => Ok(BidType::Banner),
                        "video" => Ok(BidType::Video),
                        "native" => Ok(BidType::Native),
                        "audio" => Ok(BidType::Audio),
                        other => Err(BidderError::BadServerResponse(format!("unknown bid type: {}", other))),
                    };
                }
            }
        }
    }
    Err(BidderError::BadServerResponse(format!("missing media type for bid: {}", bid.id)))
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
        let mut req = request.clone();
        // Transform each imp.ext: wrap bidder content into {"akcelo": <bidder_content>}
        let mut errs = Vec::new();
        for imp in &mut req.imp {
            match imp.ext.as_ref()
                .and_then(|e| serde_json::from_value::<ExtImpBidder>(e.clone()).ok())
            {
                Some(bidder_ext) => {
                    imp.ext = Some(serde_json::json!({ "akcelo": bidder_ext.bidder }));
                }
                None => {
                    errs.push(BidderError::BadInput(format!("Unsupported imp : {}", imp.id)));
                }
            }
        }
        // Configure publisher parent account on site
        if req.site.is_none() {
            req.site = Some(openrtb::Site::default());
        }
        if let Some(site) = &mut req.site {
            if site.publisher.is_none() {
                site.publisher = Some(openrtb::Publisher::default());
            }
            if let Some(publisher) = &mut site.publisher {
                publisher.ext = Some(serde_json::json!({
                    "prebid": { "parentAccount": site_id }
                }));
            }
        }
        let body = match serde_json::to_vec(&req) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
        (vec![RequestData { method: "POST".to_string(), uri: self.endpoint.clone(), body, headers, imp_ids: get_imp_ids(&request.imp) }], errs)
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(5);
        let mut errs = Vec::new();
        for sb in &bid_resp.seatbid {
            for bid in &sb.bid {
                match get_bid_type(bid) {
                    Ok(bt) => result.bids.push(TypedBid::new(bid.clone(), bt)),
                    Err(e) => errs.push(e),
                }
            }
        }
        if !errs.is_empty() {
            return Err(errs);
        }
        Ok(result)
    }
}
