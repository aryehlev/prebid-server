use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use openrtb_ext::BidType;

pub struct CpmstarAdapter { pub endpoint: String }
impl CpmstarAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

/// Preprocess the request: flatten imp.ext.bidder fields to imp.ext root level.
fn preprocess(request: &mut openrtb::BidRequest) -> Result<(), BidderError> {
    if request.imp.is_empty() {
        return Err(BidderError::BadInput("No Imps in Bid Request".to_string()));
    }

    for imp in &mut request.imp {
        if imp.banner.is_none() && imp.video.is_none() {
            return Err(BidderError::BadInput(
                "Only Banner and Video bid-types are supported at this time".to_string()
            ));
        }

        let ext_val = imp.ext.as_ref().ok_or_else(|| {
            BidderError::BadInput("bidder field not found in impression extension".to_string())
        })?.clone();

        let mut original_ext: HashMap<String, serde_json::Value> = serde_json::from_value(ext_val)
            .map_err(|e| BidderError::BadInput(e.to_string()))?;

        let bidder_raw = original_ext.remove("bidder").ok_or_else(|| {
            BidderError::BadInput("bidder field not found in impression extension".to_string())
        })?;

        // Flatten bidder fields into root ext
        let bidder_config: HashMap<String, serde_json::Value> = serde_json::from_value(bidder_raw)
            .map_err(|e| BidderError::BadInput(e.to_string()))?;

        for (key, value) in bidder_config {
            original_ext.insert(key, value);
        }

        imp.ext = Some(serde_json::to_value(&original_ext)
            .map_err(|e| BidderError::BadInput(e.to_string()))?);
    }

    Ok(())
}

impl Bidder for CpmstarAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut req_copy = request.clone();
        if let Err(e) = preprocess(&mut req_copy) {
            return (vec![], vec![e]);
        }

        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());

        let imp_ids: Vec<String> = req_copy.imp.iter().map(|i| i.id.clone()).collect();
        (vec![RequestData {
            method: "POST".to_string(),
            uri: self.endpoint.clone(),
            body,
            headers,
            imp_ids,
        }], vec![])
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected HTTP status code: {}. Run with request.debug = 1 for more info", response.status_code
            ))]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        if bid_resp.seatbid.is_empty() {
            return Ok(BidderResponse::new());
        }

        let capacity = bid_resp.seatbid.first().map(|sb| sb.bid.len()).unwrap_or(0);
        let mut result = BidderResponse::with_capacity(capacity);
        let mut errs = Vec::new();

        for sb in &bid_resp.seatbid {
            for bid in &sb.bid {
                let mut found = false;
                let mut bid_type = BidType::Banner;
                for imp in &internal.imp {
                    if imp.id == bid.impid {
                        found = true;
                        bid_type = if imp.banner.is_some() {
                            BidType::Banner
                        } else if imp.video.is_some() {
                            BidType::Video
                        } else {
                            BidType::Banner
                        };
                        break;
                    }
                }
                if found {
                    result.bids.push(TypedBid::new(bid.clone(), bid_type));
                } else {
                    errs.push(BidderError::BadServerResponse(format!(
                        "bid id='{}' could not find valid impid='{}'", bid.id, bid.impid
                    )));
                }
            }
        }

        Ok(result)
    }
}
