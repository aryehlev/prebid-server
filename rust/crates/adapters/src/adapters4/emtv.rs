use std::collections::HashMap;
use pbs_adapters::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_bid_type_from_imp, get_imp_ids};
use openrtb_ext::BidType;

pub struct EmtvAdapter {
    pub endpoint: String,
}

impl EmtvAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

#[derive(serde::Deserialize, Default)]
struct EmtvBidderExt {
    #[serde(rename = "placementId", default)]
    placement_id: String,
    #[serde(rename = "endpointId", default)]
    endpoint_id: String,
}

#[derive(serde::Deserialize)]
struct ExtImpBidder {
    bidder: EmtvBidderExt,
}

#[derive(serde::Serialize)]
struct ReqBodyExtBidder {
    #[serde(rename = "type")]
    bidder_type: String,
    #[serde(rename = "placementId", skip_serializing_if = "String::is_empty")]
    placement_id: String,
    #[serde(rename = "endpointId", skip_serializing_if = "String::is_empty")]
    endpoint_id: String,
}

#[derive(serde::Serialize)]
struct ReqBodyExt {
    bidder: ReqBodyExtBidder,
}

impl Bidder for EmtvAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();
        let mut errors = Vec::new();

        for imp in &request.imp {
            // Parse bidder ext.
            let emtv_ext = match imp.ext.as_ref()
                .and_then(|e| serde_json::from_str::<ExtImpBidder>(e.get()).ok())
            {
                Some(e) => e.bidder,
                None => {
                    errors.push(BidderError::BadInput(
                        "Failed to parse emtv imp ext".to_string(),
                    ));
                    continue;
                }
            };

            let outgoing_ext = if !emtv_ext.placement_id.is_empty() {
                ReqBodyExt {
                    bidder: ReqBodyExtBidder {
                        bidder_type: "publisher".to_string(),
                        placement_id: emtv_ext.placement_id,
                        endpoint_id: String::new(),
                    },
                }
            } else if !emtv_ext.endpoint_id.is_empty() {
                ReqBodyExt {
                    bidder: ReqBodyExtBidder {
                        bidder_type: "network".to_string(),
                        placement_id: String::new(),
                        endpoint_id: emtv_ext.endpoint_id,
                    },
                }
            } else {
                // Neither placementId nor endpointId — skip this imp.
                continue;
            };

            let ext_json = match serde_json::to_value(&outgoing_ext) {
                Ok(v) => v,
                Err(e) => {
                    errors.push(BidderError::BadInput(e.to_string()));
                    continue;
                }
            };

            let mut imp_copy = imp.clone();
            imp_copy.ext = Some(ext_json);

            let mut req_copy = request.clone();
            req_copy.imp = vec![imp_copy];

            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => {
                    errors.push(BidderError::BadInput(e.to_string()));
                    continue;
                }
            };

            let mut headers = HashMap::new();
            headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
            headers.insert("Accept".to_string(), "application/json".to_string());

            requests.push(RequestData {
                method: "POST".to_string(),
                uri: self.endpoint.clone(),
                body,
                headers,
                imp_ids: get_imp_ids(&req_copy.imp),
            });
        }

        if requests.is_empty() && errors.is_empty() {
            errors.push(BidderError::BadInput("found no valid impressions".to_string()));
        }

        (requests, errors)
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
        if let Err(e) = pbs_adapters::check_response_status(response.status_code) {
            return Err(vec![e]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(5);
        if let Some(cur) = &bid_resp.cur {
            result.currency = cur.clone();
        }

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
