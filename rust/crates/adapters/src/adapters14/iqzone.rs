use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde_json::Value;

pub struct IqzoneAdapter {
    pub endpoint: String,
}

impl IqzoneAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

fn get_bid_media_type_from_mtype(mtype: u32, imp_id: &str) -> Result<BidType, BidderError> {
    match mtype {
        1 => Ok(BidType::Banner),
        2 => Ok(BidType::Video),
        4 => Ok(BidType::Native),
        _ => Err(BidderError::BadServerResponse(format!(
            "Unable to fetch mediaType in multi-format: {}", imp_id
        ))),
    }
}

impl Bidder for IqzoneAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();
        let mut errs = Vec::new();

        for imp in &request.imp {
            let bidder_ext = match imp.ext.as_ref().and_then(|e| e.get("bidder")) {
                Some(b) => b.clone(),
                None => {
                    errs.push(BidderError::BadInput(format!(
                        "imp {} missing ext.bidder", imp.id
                    )));
                    continue;
                }
            };

            let placement_id = bidder_ext.get("placementId").and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_default();
            let endpoint_id = bidder_ext.get("endpointId").and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_default();

            // Build the final imp ext based on placementId or endpointId
            let final_imp_ext: Value = if !placement_id.is_empty() {
                serde_json::json!({
                    "bidder": {
                        "placementId": placement_id,
                        "type": "publisher"
                    }
                })
            } else if !endpoint_id.is_empty() {
                serde_json::json!({
                    "bidder": {
                        "endpointId": endpoint_id,
                        "type": "network"
                    }
                })
            } else {
                // Keep original ext if neither
                imp.ext.clone().unwrap_or(Value::Null)
            };

            let mut imp_copy = imp.clone();
            imp_copy.ext = Some(final_imp_ext);

            let mut req = request.clone();
            req.imp = vec![imp_copy];

            let body = match serde_json::to_vec(&req) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(BidderError::BadInput(e.to_string()));
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
                imp_ids: get_imp_ids(&req.imp),
            });
        }

        (requests, errs)
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
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info.",
                response.status_code
            ))]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(5);
        if let Some(cur) = &bid_resp.cur {
            if !cur.is_empty() {
                result.currency = cur.clone();
            }
        }

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let mtype = bid.ext.as_ref()
                    .and_then(|e| e.get("mtype"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32;
                let bid_type = get_bid_media_type_from_mtype(mtype, &bid.impid)
                    .map_err(|e| vec![e])?;
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }

        Ok(result)
    }
}
