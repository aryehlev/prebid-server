use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;
use serde_json::Value;

pub struct AdviewAdapter { pub endpoint: String }
impl AdviewAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

#[derive(Deserialize)]
struct ExtImpBidder { bidder: Value }

#[derive(Deserialize)]
struct ExtImpAdView {
    #[serde(rename = "placementId", default)]
    master_tag_id: String,
    #[serde(rename = "accountId", default)]
    account_id: String,
}

impl Bidder for AdviewAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut requests = Vec::new();
        let mut req = request.clone();
        for imp in &request.imp {
            let bidder_ext: ExtImpBidder = match imp.ext.as_ref()
                .and_then(|e| serde_json::from_value(e.clone()).ok()) {
                Some(v) => v,
                None => { errs.push(BidderError::BadInput(format!("invalid imp.ext for imp {}", imp.id))); continue; }
            };
            let ext: ExtImpAdView = match serde_json::from_value(bidder_ext.bidder) {
                Ok(v) => v,
                Err(e) => { errs.push(BidderError::BadInput(format!("invalid bidderExt.Bidder, {}", e))); continue; }
            };
            let uri = self.endpoint.replace("{{.AccountID}}", &ext.account_id);
            let mut imp_copy = imp.clone();
            imp_copy.tagid = Some(ext.master_tag_id);
            // For adview, set banner w/h from first format
            if let Some(banner) = &imp_copy.banner {
                if let Some(formats) = &banner.format {
                    if !formats.is_empty() {
                        let mut b = banner.clone();
                        b.h = formats[0].h;
                        b.w = formats[0].w;
                        imp_copy.banner = Some(b);
                    }
                }
            }
            req.imp = vec![imp_copy];
            req.cur = Some(vec!["USD".to_string()]);
            let body = match serde_json::to_vec(&req) {
                Ok(b) => b,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };
            requests.push(RequestData { method: "POST".to_string(), uri, body, headers: HashMap::new(), imp_ids: get_imp_ids(&req.imp) });
        }
        (requests, errs)
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!("Unexpected status code: {}. Run with request.debug = 1 for more info.", response.status_code))]);
        }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(5);
        if let Some(cur) = &bid_resp.cur { result.currency = cur.clone(); }
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                result.bids.push(TypedBid::new(bid, BidType::Banner));
            }
        }
        Ok(result)
    }
}
