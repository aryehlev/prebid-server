use std::collections::HashMap;
use pbs_adapters::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct DeepintentAdapter {
    pub endpoint: String,
}

impl DeepintentAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

const DISPLAY_MANAGER: &str = "di_prebid";
const DISPLAY_MANAGER_VER: &str = "2.0.0";

/// Minimal bidder ext for Deepintent.
#[derive(serde::Deserialize)]
struct ExtImpBidder {
    bidder: DeepintentExt,
}

#[derive(serde::Deserialize)]
struct DeepintentExt {
    #[serde(rename = "tagId", default)]
    tag_id: String,
}

impl Bidder for DeepintentAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();
        let mut errors = Vec::new();

        for imp in &request.imp {
            // Parse bidder extension to get tagId.
            let deepintent_ext = match imp.ext.as_ref()
                .and_then(|e| serde_json::from_str::<ExtImpBidder>(e.get()).ok())
            {
                Some(e) => e.bidder,
                None => {
                    errors.push(BidderError::BadInput(
                        format!("Impression id={} has an invalid ext", imp.id),
                    ));
                    continue;
                }
            };

            // Deepintent requires a banner object.
            if imp.banner.is_none() {
                errors.push(BidderError::BadInput(
                    "We need a Banner Object in the request".to_string(),
                ));
                continue;
            }

            let mut imp_copy = imp.clone();
            imp_copy.tagid = Some(deepintent_ext.tag_id);
            imp_copy.displaymanager = Some(DISPLAY_MANAGER.to_string());
            imp_copy.displaymanagerver = Some(DISPLAY_MANAGER_VER.to_string());

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

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                // Deepintent always returns banner bids — bid type resolved from imp.
                let found = internal.imp.iter().any(|i| i.id == bid.impid);
                if found {
                    result.bids.push(TypedBid::new(bid, BidType::Banner));
                } else {
                    // No matching impression — skip this bid (log as error in Go).
                }
            }
        }
        Ok(result)
    }
}
