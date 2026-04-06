use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use openrtb_ext::BidType;
use serde::Deserialize;
use serde_json::Value;

pub struct BidscubeAdapter { pub endpoint: String }
impl BidscubeAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

#[derive(Deserialize)]
struct ExtBidPrebid {
    #[serde(rename = "type", default)]
    bid_type: String,
}

#[derive(Deserialize)]
struct BidExtPrebidWrapper {
    prebid: Option<ExtBidPrebid>,
}

fn get_media_type_from_str(bid_type: &str) -> BidType {
    match bid_type {
        "banner" => BidType::Banner,
        "video"  => BidType::Video,
        "native" => BidType::Native,
        _        => BidType::Banner,
    }
}

impl Bidder for BidscubeAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        let mut results = Vec::new();
        let mut errs = Vec::new();

        for imp in &request.imp {
            // Extract bidder ext
            let ext_map: std::collections::HashMap<String, Value> = match imp.ext.as_ref()
                .and_then(|e| serde_json::from_value(e.clone()).ok())
            {
                Some(m) => m,
                None => { errs.push(BidderError::BadInput("bidder parameters required".to_string())); continue; }
            };

            let bidder_ext = match ext_map.get("bidder") {
                Some(v) if !v.is_null() => v.clone(),
                _ => { errs.push(BidderError::BadInput("bidder parameters required".to_string())); continue; }
            };

            let mut new_imp = imp.clone();
            new_imp.ext = Some(bidder_ext);

            let mut req = request.clone();
            req.imp = vec![new_imp];

            let body = match serde_json::to_vec(&req) {
                Ok(b) => b,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };

            results.push(RequestData {
                method: "POST".to_string(),
                uri: self.endpoint.clone(),
                body,
                headers: headers.clone(),
                imp_ids: vec![imp.id.clone()],
            });
        }

        (results, errs)
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code == 400 {
            return Err(vec![BidderError::BadInput(format!("unexpected status code: {}", response.status_code))]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!("unexpected status code: {}", response.status_code))]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(internal.imp.len());
        let mut errs = Vec::new();

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = bid.ext.as_ref()
                    .and_then(|e| serde_json::from_value::<BidExtPrebidWrapper>(e.clone()).ok())
                    .and_then(|w| w.prebid)
                    .map(|p| get_media_type_from_str(&p.bid_type));

                match bid_type {
                    Some(t) => result.bids.push(TypedBid::new(bid, t)),
                    None => errs.push(BidderError::BadServerResponse(
                        format!("unable to read bid.ext.prebid.type")
                    )),
                }
            }
        }

        Ok(result)
    }
}
