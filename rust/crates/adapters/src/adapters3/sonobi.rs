use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct SonobiAdapter {
    pub endpoint: String,
}

impl SonobiAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

fn get_media_type_for_imp(imp_id: &str, imps: &[openrtb::Imp]) -> Result<BidType, BidderError> {
    for imp in imps {
        if imp.id == imp_id {
            // Banner is default; only override if banner is absent
            if imp.banner.is_none() && imp.video.is_some() {
                return Ok(BidType::Video);
            }
            if imp.banner.is_none() && imp.video.is_none() && imp.native.is_some() {
                return Ok(BidType::Native);
            }
            return Ok(BidType::Banner);
        }
    }
    Err(BidderError::BadInput(format!(
        "Failed to find impression \"{}\" ",
        imp_id
    )))
}

impl Bidder for SonobiAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut adapter_requests = Vec::new();

        // Sonobi only supports 1 imp per request — send one request per imp
        for imp in &request.imp {
            let mut req_copy = request.clone();
            req_copy.imp = vec![imp.clone()];

            // Extract tagId from imp.ext.bidder.tagId
            let tag_id = req_copy.imp[0]
                .ext
                .as_ref()
                .and_then(|e| e.get("bidder"))
                .and_then(|b| b.get("tagId").or_else(|| b.get("tag_id")))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            if !tag_id.is_empty() {
                req_copy.imp[0].tagid = Some(tag_id);
            }

            // Sonobi only bids in USD
            req_copy.cur = Some(vec!["USD".to_string()]);

            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(BidderError::BadInput(e.to_string()));
                    continue;
                }
            };

            let mut headers = HashMap::new();
            headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
            headers.insert("Accept".to_string(), "application/json".to_string());

            let imp_ids = get_imp_ids(&req_copy.imp);
            adapter_requests.push(RequestData {
                method: "POST".to_string(),
                uri: self.endpoint.clone(),
                body,
                headers,
                imp_ids,
            });
        }

        (adapter_requests, errs)
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
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(5);
        result.currency = "USD".to_string(); // Sonobi only bids in USD

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = get_media_type_for_imp(&bid.impid, &internal.imp)
                    .map_err(|e| vec![e])?;
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }

        Ok(result)
    }
}
