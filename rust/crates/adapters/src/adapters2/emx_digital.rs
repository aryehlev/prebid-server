use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids, get_bid_type_from_imp};
use serde::{Deserialize, Serialize};

/// EmxDigital adapter — matches the `emtv` Go adapter which this was ported from.
/// Per the Go source (adapters/emtv/emtv.go), each impression is sent as a separate
/// request with its ext rewritten to include a `type` field ("publisher" or "network").
pub struct EmxDigitalAdapter {
    pub endpoint: String,
}

impl EmxDigitalAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

#[derive(Debug, Default, Deserialize)]
struct ExtImpEmtv {
    #[serde(rename = "placementId", default)]
    placement_id: String,
    #[serde(rename = "endpointId", default)]
    endpoint_id: String,
}

#[derive(Debug, Serialize)]
struct ReqBodyExtBidder {
    #[serde(rename = "type")]
    ext_type: String,
    #[serde(rename = "placementId", skip_serializing_if = "String::is_empty")]
    placement_id: String,
    #[serde(rename = "endpointId", skip_serializing_if = "String::is_empty")]
    endpoint_id: String,
}

#[derive(Debug, Serialize)]
struct ReqBodyExt {
    bidder: ReqBodyExtBidder,
}

impl Bidder for EmxDigitalAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut adapter_requests = Vec::new();
        let mut errors = Vec::new();

        for imp in &request.imp {
            // Parse bidder ext
            let bidder_val = match imp.ext.as_ref().and_then(|e| e.get("bidder")).cloned() {
                Some(v) => v,
                None => {
                    errors.push(BidderError::BadInput(format!(
                        "imp {}: missing bidder ext", imp.id
                    )));
                    return (vec![], errors);
                }
            };

            let emtv_ext: ExtImpEmtv = match serde_json::from_value(bidder_val) {
                Ok(e) => e,
                Err(e) => {
                    errors.push(BidderError::BadInput(e.to_string()));
                    return (vec![], errors);
                }
            };

            // Determine type based on which ID is provided
            let rewritten_bidder = if !emtv_ext.placement_id.is_empty() {
                ReqBodyExtBidder {
                    ext_type: "publisher".to_string(),
                    placement_id: emtv_ext.placement_id,
                    endpoint_id: String::new(),
                }
            } else if !emtv_ext.endpoint_id.is_empty() {
                ReqBodyExtBidder {
                    ext_type: "network".to_string(),
                    placement_id: String::new(),
                    endpoint_id: emtv_ext.endpoint_id,
                }
            } else {
                // No valid IDs — skip this impression (matches Go behaviour: `continue`)
                continue;
            };

            let imp_ext = ReqBodyExt { bidder: rewritten_bidder };
            let imp_ext_json = match serde_json::to_value(&imp_ext) {
                Ok(v) => v,
                Err(e) => {
                    errors.push(BidderError::BadInput(e.to_string()));
                    return (vec![], errors);
                }
            };

            // Build a single-imp copy with rewritten ext
            let mut imp_copy = imp.clone();
            imp_copy.ext = Some(imp_ext_json);

            let mut req_copy = request.clone();
            req_copy.imp = vec![imp_copy];

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

            adapter_requests.push(RequestData {
                method: "POST".to_string(),
                uri: self.endpoint.clone(),
                body,
                headers,
                imp_ids: get_imp_ids(&req_copy.imp),
            });
        }

        if adapter_requests.is_empty() && errors.is_empty() {
            errors.push(BidderError::BadInput("found no valid impressions".to_string()));
        }

        (adapter_requests, errors)
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

        let bid_response: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(internal.imp.len());
        if let Some(cur) = bid_response.cur.as_deref() {
            if !cur.is_empty() {
                result.currency = cur.to_string();
            }
        }

        // Build imp map for O(1) lookup
        let imp_map: HashMap<&str, &openrtb::Imp> =
            internal.imp.iter().map(|i| (i.id.as_str(), i)).collect();

        for sb in bid_response.seatbid {
            for bid in sb.bid {
                let bid_type = imp_map
                    .get(bid.impid.as_str())
                    .map(|imp| get_bid_type_from_imp(imp))
                    .unwrap_or(openrtb_ext::BidType::Banner);
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }

        Ok(result)
    }
}
