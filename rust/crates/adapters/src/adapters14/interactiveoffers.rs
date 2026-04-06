use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct InteractiveoffersAdapter { pub endpoint: String }
impl InteractiveoffersAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

impl Bidder for InteractiveoffersAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![]);
        }

        // Extract partnerId from first imp.ext.bidder
        let bidder_ext = request.imp[0].ext.as_ref()
            .and_then(|e| e.get("bidder"))
            .cloned();

        let partner_id = bidder_ext
            .as_ref()
            .and_then(|b| b.get("partnerId"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // If bidder ext is missing, fail with BadInput
        if bidder_ext.is_none() {
            return (vec![], vec![BidderError::BadInput("missing bidder ext".to_string())]);
        }

        // Build URL from template: replace {{.AccountID}} with partner_id
        let url = self.endpoint.replace("{{.AccountID}}", partner_id);

        let body = match serde_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        (vec![RequestData {
            method: "POST".to_string(),
            uri: url,
            body,
            headers,
            imp_ids: get_imp_ids(&request.imp),
        }], vec![])
    }

    fn make_bids(&self, request: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code == 400 {
            return Err(vec![BidderError::BadInput(
                "Unexpected status code: 400. Bad request from publisher. Run with request.debug = 1 for more info.".to_string()
            )]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info.",
                response.status_code
            ))]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(request.imp.len());
        if let Some(cur) = &bid_resp.cur {
            result.currency = cur.clone();
        }

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                // interactiveoffers only serves banner ads
                result.bids.push(TypedBid::new(bid, BidType::Banner));
            }
        }
        Ok(result)
    }
}
