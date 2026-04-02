use std::collections::HashMap;
use pbs_adapters::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct LockerdomeAdapter {
    pub endpoint: String,
}

impl LockerdomeAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

impl Bidder for LockerdomeAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errors = Vec::new();

        if request.imp.is_empty() {
            errors.push(BidderError::BadInput("No valid impressions in the bid request.".to_string()));
            return (vec![], errors);
        }

        // LockerDome only supports banner impressions
        let valid_imps: Vec<openrtb::Imp> = request
            .imp
            .iter()
            .filter(|imp| {
                if imp.banner.is_none() {
                    errors.push(BidderError::BadInput(
                        "LockerDome does not currently support non-banner types.".to_string(),
                    ));
                    false
                } else {
                    true
                }
            })
            .cloned()
            .collect();

        if valid_imps.is_empty() {
            errors.push(BidderError::BadInput(
                "No valid or supported impressions in the bid request.".to_string(),
            ));
            return (vec![], errors);
        }

        let mut req_copy = request.clone();
        req_copy.imp = valid_imps;

        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => {
                errors.push(BidderError::BadInput(e.to_string()));
                return (vec![], errors);
            }
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
        headers.insert("x-openrtb-version".to_string(), "2.5".to_string());

        let imp_ids = get_imp_ids(&req_copy.imp);
        (
            vec![RequestData {
                method: "POST".to_string(),
                uri: self.endpoint.clone(),
                body,
                headers,
                imp_ids,
            }],
            errors,
        )
    }

    fn make_bids(
        &self,
        _internal: &openrtb::BidRequest,
        _external: &RequestData,
        response: &ResponseData,
    ) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 {
            return Ok(BidderResponse::new());
        }
        if let Err(e) = pbs_adapters::check_response_status(response.status_code) {
            return Err(vec![e]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        if bid_resp.seatbid.is_empty() {
            return Ok(BidderResponse::new());
        }

        let mut result = BidderResponse::with_capacity(bid_resp.seatbid[0].bid.len());
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                result.bids.push(TypedBid::new(bid, BidType::Banner));
            }
        }

        Ok(result)
    }
}
