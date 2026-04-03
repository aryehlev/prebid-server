use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_bid_type_from_imp, get_imp_ids};
use openrtb_ext::BidType;

pub struct EpomAdapter {
    pub endpoint: String,
}

impl EpomAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

impl Bidder for EpomAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        // Epom requires a device with an IPv4 address.
        let has_ip = request
            .device
            .as_ref()
            .and_then(|d| d.ip.as_deref())
            .map(|ip| !ip.is_empty())
            .unwrap_or(false);

        if !has_ip {
            return (
                vec![],
                vec![BidderError::BadInput("ipv4 address is required field".to_string())],
            );
        }

        let body = match serde_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        (
            vec![RequestData {
                method: "POST".to_string(),
                uri: self.endpoint.clone(),
                body,
                headers,
                imp_ids: get_imp_ids(&request.imp),
            }],
            vec![],
        )
    }

    fn make_bids(
        &self,
        internal: &openrtb::BidRequest,
        _external: &RequestData,
        response: &ResponseData,
    ) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 {
            return Ok(BidderResponse::new());
        }
        if response.status_code >= 500 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected status code: {}. Dsp server internal error",
                response.status_code
            ))]);
        }
        if response.status_code >= 400 {
            return Err(vec![BidderError::BadInput(format!(
                "Unexpected status code: {}. Bad request to dsp",
                response.status_code
            ))]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected status code: {}",
                response.status_code
            ))]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        // Additional no-content check.
        let has_bids = bid_resp
            .seatbid
            .first()
            .map(|sb| !sb.bid.is_empty())
            .unwrap_or(false);
        if !has_bids {
            return Err(vec![BidderError::Warning("No bids in response".to_string())]);
        }

        let mut result = BidderResponse::with_capacity(5);

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = internal
                    .imp
                    .iter()
                    .find(|i| i.id == bid.impid)
                    .map(get_bid_type_from_imp)
                    .unwrap_or(BidType::Banner);
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        Ok(result)
    }
}
