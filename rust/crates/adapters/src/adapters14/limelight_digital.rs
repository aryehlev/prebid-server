use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use openrtb_ext::BidType;
use serde::Deserialize;

pub struct LimelightDigitalAdapter { pub endpoint: String }
impl LimelightDigitalAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

#[derive(Debug, Deserialize, Clone)]
struct ImpExtLimelightDigital {
    #[serde(default)]
    host: String,
    #[serde(rename = "publisherId", default)]
    publisher_id: String,
}

fn get_media_type_for_bid(imp_id: &str, imps: &[openrtb::Imp]) -> Result<BidType, BidderError> {
    for imp in imps {
        if imp.id == imp_id {
            if imp.banner.is_some() { return Ok(BidType::Banner); }
            if imp.video.is_some() { return Ok(BidType::Video); }
            if imp.audio.is_some() { return Ok(BidType::Audio); }
            if imp.native.is_some() { return Ok(BidType::Native); }
            return Err(BidderError::BadServerResponse(format!("unknown media type of imp: {}", imp_id)));
        }
    }
    Err(BidderError::BadServerResponse(format!("bid contains unknown imp id: {}", imp_id)))
}

impl Bidder for LimelightDigitalAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();
        let mut errs = Vec::new();

        for imp in &request.imp {
            // Parse bidder ext
            let bidder_val = match imp.ext.as_ref().and_then(|e| e.get("bidder")) {
                Some(v) => v.clone(),
                None => {
                    errs.push(BidderError::BadInput("ext.bidder is not provided".to_string()));
                    continue;
                }
            };
            let ll_ext: ImpExtLimelightDigital = match serde_json::from_value(bidder_val) {
                Ok(e) => e,
                Err(_) => {
                    errs.push(BidderError::BadInput("ext.bidder is not provided".to_string()));
                    continue;
                }
            };

            // Build URL: replace {{.Host}} and {{.PublisherID}} macros
            let url = self.endpoint
                .replace("{{.Host}}", &ll_ext.host)
                .replace("{{.PublisherID}}", &ll_ext.publisher_id);

            // Build request with single imp (clear ext)
            let mut imp_copy = imp.clone();
            imp_copy.ext = None;

            let mut req_copy = request.clone();
            req_copy.id = format!("{}-{}", request.id, imp.id);
            req_copy.imp = vec![imp_copy];
            req_copy.ext = None;

            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(BidderError::BadInput(e.to_string()));
                    continue;
                }
            };

            requests.push(RequestData {
                method: "POST".to_string(),
                uri: url,
                body,
                headers: HashMap::new(),
                imp_ids: vec![imp.id.clone()],
            });
        }

        (requests, errs)
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code == 400 {
            return Err(vec![BidderError::BadInput(
                "Unexpected status code: 400. Bad request from publisher. Run with request.debug = 1 for more info.".to_string()
            )]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info.", response.status_code
            ))]);
        }
        if response.body.is_empty() {
            return Ok(BidderResponse::new());
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(internal.imp.len());
        if let Some(cur) = &bid_resp.cur {
            result.currency = cur.clone();
        }

        let mut errs = Vec::new();
        for sb in &bid_resp.seatbid {
            for (i, bid) in sb.bid.iter().enumerate() {
                match get_media_type_for_bid(&bid.impid, &internal.imp) {
                    Ok(bid_type) => {
                        result.bids.push(TypedBid::new(sb.bid[i].clone(), bid_type));
                    }
                    Err(e) => {
                        errs.push(e);
                    }
                }
            }
        }

        Ok(result)
    }
}
