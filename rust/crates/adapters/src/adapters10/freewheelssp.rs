use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids, check_response_status};
use openrtb::BidResponse;
use openrtb_ext::{BidType, ExtBidPrebidVideo};

pub struct FreewheelsspAdapter { pub endpoint: String }
impl FreewheelsspAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

impl Bidder for FreewheelsspAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut req = request.clone();

        // For each imp, replace ext with only the bidder inner fields (strip bidder wrapper)
        for (i, imp) in req.imp.iter_mut().enumerate() {
            let bidder_ext = imp.ext.as_ref()
                .and_then(|e| e.get("bidder"))
                .cloned();

            match bidder_ext {
                Some(inner) => {
                    imp.ext = Some(inner);
                }
                None => {
                    return (vec![], vec![BidderError::BadInput(format!(
                        "Invalid imp.ext for impression index {}. Error Infomation: missing bidder ext", i
                    ))]);
                }
            }
        }

        let body = match serde_json::to_vec(&req) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(format!(
                "Unable to transfer request to Json fomat, {}", e
            ))]),
        };

        let mut headers = HashMap::new();
        headers.insert("Componentid".to_string(), "prebid-go".to_string());

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
        if let Err(e) = check_response_status(response.status_code) { return Err(vec![e]); }

        let bid_resp: BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::new();
        if let Some(cur) = &bid_resp.cur {
            if !cur.is_empty() { result.currency = cur.clone(); }
        }

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let mut bid_video = ExtBidPrebidVideo::default();
                if let Some(cats) = &bid.cat {
                    if let Some(first) = cats.first() {
                        bid_video.primary_category = first.clone();
                    }
                }
                let mut typed_bid = TypedBid::new(bid, BidType::Video);
                typed_bid.bid_video = Some(bid_video);
                result.bids.push(typed_bid);
            }
        }
        Ok(result)
    }
}
