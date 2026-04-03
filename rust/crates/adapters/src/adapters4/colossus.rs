use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_bid_type_from_imp, get_imp_ids};
use openrtb_ext::BidType;

pub struct ColossusAdapter {
    pub endpoint: String,
}

impl ColossusAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

/// Colossus bid extension used to determine media type from server response.
#[derive(serde::Deserialize, Default)]
struct ColossusBidExt {
    #[serde(rename = "mediaType", default)]
    media_type: String,
}

impl Bidder for ColossusAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();
        let mut errors = Vec::new();

        // Colossus sends one request per impression, optionally setting TagID from bidder ext.
        for imp in &request.imp {
            let mut req_copy = request.clone();

            // Attempt to extract TagID from imp ext bidder params.
            let tag_id = imp.ext.as_ref().and_then(|ext| {
                let v: serde_json::Value = serde_json::from_str(ext.get()).ok()?;
                v.get("bidder")?.get("TagID")?.as_str().map(|s| s.to_string())
            });

            let mut imp_copy = imp.clone();
            if let Some(tid) = tag_id {
                imp_copy.tagid = Some(tid);
            }

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
        if let Err(e) = crate::check_response_status(response.status_code) {
            return Err(vec![e]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(5);
        let mut errors = Vec::new();

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                // Try to determine bid type from the bid ext mediaType field first.
                let bid_type = if let Some(ext_raw) = &bid.ext {
                    if let Ok(ext) = serde_json::from_str::<ColossusBidExt>(ext_raw.get()) {
                        match ext.media_type.as_str() {
                            "banner" => Some(BidType::Banner),
                            "video" => Some(BidType::Video),
                            "native" => Some(BidType::Native),
                            _ => None,
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };

                let bid_type = bid_type.unwrap_or_else(|| {
                    internal
                        .imp
                        .iter()
                        .find(|i| i.id == bid.impid)
                        .map(get_bid_type_from_imp)
                        .unwrap_or(BidType::Banner)
                });

                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }

        if errors.is_empty() {
            Ok(result)
        } else {
            // Return partial result with errors — match Go behaviour of appending both.
            Ok(result)
        }
    }
}
