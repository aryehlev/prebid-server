use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_bid_type_from_imp, get_imp_ids};
use openrtb_ext::BidType;
use serde::{Deserialize, Serialize};

pub struct SharethroughAdapter {
    pub endpoint: String,
}

impl SharethroughAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

#[derive(Serialize, Deserialize, Default)]
struct ExtImpSharethrough {
    #[serde(rename = "pkey", default)]
    pub pkey: String,
}

#[derive(Deserialize, Default)]
struct ImpExt {
    #[serde(default)]
    bidder: ExtImpSharethrough,
}

impl Bidder for SharethroughAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();
        let mut errs = Vec::new();

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        for imp in &request.imp {
            let imp_ext: ImpExt = match imp.ext.as_ref().and_then(|e| serde_json::from_value(e.clone()).ok()) {
                Some(e) => e,
                None => {
                    errs.push(BidderError::BadInput("Failed to parse sharethrough imp ext".to_string()));
                    continue;
                }
            };

            let pkey = imp_ext.bidder.pkey.clone();

            // Build per-imp request
            let mut req_copy = request.clone();
            req_copy.imp = vec![imp.clone()];

            // Set tagid from pkey
            req_copy.imp[0].tagid = if pkey.is_empty() { None } else { Some(pkey.clone()) };

            let uri = if pkey.is_empty() {
                self.endpoint.clone()
            } else {
                format!("{}?pkey={}", self.endpoint, pkey)
            };

            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(BidderError::BadInput(e.to_string()));
                    continue;
                }
            };

            let imp_ids = get_imp_ids(&req_copy.imp);

            requests.push(RequestData {
                method: "POST".to_string(),
                uri,
                body,
                headers: headers.clone(),
                imp_ids,
            });
        }

        (requests, errs)
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
        if let Err(e) = crate::check_response_status(response.status_code) {
            return Err(vec![e]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

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
