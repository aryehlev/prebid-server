use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
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

        let mut valid_imp_indices: Vec<usize> = Vec::new();

        for (i, imp) in request.imp.iter().enumerate() {
            // LockerDome only supports banner impressions
            if imp.banner.is_none() {
                errors.push(BidderError::BadInput(
                    "LockerDome does not currently support non-banner types.".to_string(),
                ));
                continue;
            }

            // Validate ext exists
            let bidder_val = match imp.ext.as_ref().and_then(|e| e.get("bidder")).cloned() {
                Some(v) => v,
                None => {
                    errors.push(BidderError::BadInput("ext was not provided.".to_string()));
                    continue;
                }
            };

            // Validate adUnitId is present and non-empty
            let ad_unit_id = bidder_val
                .get("adUnitId")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            if ad_unit_id.is_empty() {
                // Distinguish missing key vs empty value (both produce bad input errors in Go)
                if bidder_val.get("adUnitId").is_none() {
                    errors.push(BidderError::BadInput(
                        "ext.bidder.adUnitId was not provided.".to_string(),
                    ));
                } else {
                    errors.push(BidderError::BadInput(
                        "ext.bidder.adUnitId is empty.".to_string(),
                    ));
                }
                continue;
            }

            valid_imp_indices.push(i);
        }

        if valid_imp_indices.is_empty() {
            errors.push(BidderError::BadInput(
                "No valid or supported impressions in the bid request.".to_string(),
            ));
            return (vec![], errors);
        }

        // Build request with only valid imps (preserving original indices as Go does)
        let mut req_copy = request.clone();
        req_copy.imp = valid_imp_indices
            .iter()
            .map(|&i| request.imp[i].clone())
            .collect();

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

        // 400 → BadInput, other non-200 → BadServerResponse (matches Go)
        if response.status_code == 400 {
            return Err(vec![BidderError::BadInput(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info",
                response.status_code
            ))]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info",
                response.status_code
            ))]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(format!(
                "Error unmarshaling LockerDome bid response - {}", e
            ))])?;

        if bid_resp.seatbid.is_empty() {
            return Ok(BidderResponse::new());
        }

        let mut result = BidderResponse::with_capacity(bid_resp.seatbid[0].bid.len());
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                // LockerDome only returns banner bids
                result.bids.push(TypedBid::new(bid, BidType::Banner));
            }
        }

        Ok(result)
    }
}
