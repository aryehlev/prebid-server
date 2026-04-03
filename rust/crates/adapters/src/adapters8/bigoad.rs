use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;
use serde_json::Value;

pub struct BigoadAdapter {
    pub endpoint: String,
}

impl BigoadAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

#[derive(Deserialize)]
struct ExtImpBidder {
    bidder: Value,
}

#[derive(Deserialize)]
struct ExtImpBigoAd {
    #[serde(rename = "sspid", default)]
    ssp_id: String,
}

fn get_bid_type(imp: &openrtb::Imp, bid: &openrtb::Bid) -> Result<BidType, BidderError> {
    match bid.mtype {
        Some(1) => Ok(BidType::Banner),
        Some(2) => Ok(BidType::Video),
        Some(4) => Ok(BidType::Native),
        _ => Err(BidderError::BadInput(format!(
            "unrecognized bid type in response from bigoad {}", imp.id
        ))),
    }
}

impl Bidder for BigoadAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![]);
        }

        let imp = &request.imp[0];
        let ext_val = match &imp.ext {
            Some(v) => v.clone(),
            None => return (vec![], vec![BidderError::BadInput(format!("imp {}: unable to unmarshal ext", imp.id))]),
        };

        let bidder_ext: ExtImpBidder = match serde_json::from_value(ext_val) {
            Ok(v) => v,
            Err(_) => return (vec![], vec![BidderError::BadInput(format!("imp {}: unable to unmarshal ext", imp.id))]),
        };

        let bigoad_ext: ExtImpBigoAd = match serde_json::from_value(bidder_ext.bidder.clone()) {
            Ok(v) => v,
            Err(e) => return (vec![], vec![BidderError::BadInput(format!("imp {}: unable to unmarshal ext.bidder: {}", imp.id, e))]),
        };

        // Replace imp.ext with just the bidder ext
        let mut req_copy = request.clone();
        req_copy.imp[0].ext = Some(bidder_ext.bidder);

        let url = self.endpoint.replace("{{.SspId}}", &bigoad_ext.ssp_id);

        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
        headers.insert("x-openrtb-version".to_string(), "2.5".to_string());

        (vec![RequestData {
            method: "POST".to_string(),
            uri: url,
            body,
            headers,
            imp_ids: get_imp_ids(&request.imp),
        }], vec![])
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) {
            return Err(vec![e]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(format!("Bad server response: {}", e))])?;

        if bid_resp.seatbid.is_empty() {
            return Err(vec![BidderError::BadServerResponse("Empty SeatBid array".to_string())]);
        }

        let sb = &bid_resp.seatbid[0];
        let mut result = BidderResponse::with_capacity(sb.bid.len());
        let mut errs = Vec::new();

        let first_imp = internal.imp.first();
        for bid in &sb.bid {
            let imp = first_imp.unwrap_or_else(|| &internal.imp[0]);
            match get_bid_type(imp, bid) {
                Ok(bt) => result.bids.push(TypedBid::new(bid.clone(), bt)),
                Err(e) => errs.push(e),
            }
        }

        Ok(result)
    }
}
