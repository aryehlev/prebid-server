use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_bid_type_from_imp, get_imp_ids};
use openrtb_ext::BidType;

pub struct BeachfrontAdapter {
    pub endpoint: String,
}

impl BeachfrontAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

impl Bidder for BeachfrontAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut video_imps = Vec::new();
        let mut banner_imps = Vec::new();

        for imp in &request.imp {
            if imp.video.is_some() {
                video_imps.push(imp.clone());
            } else {
                banner_imps.push(imp.clone());
            }
        }

        let mut requests = Vec::new();
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        // Video request
        if !video_imps.is_empty() {
            let mut req_copy = request.clone();
            req_copy.imp = video_imps.clone();

            match serde_json::to_vec(&req_copy) {
                Ok(body) => {
                    let imp_ids = get_imp_ids(&video_imps);
                    requests.push(RequestData {
                        method: "POST".to_string(),
                        uri: format!("{}/bid/video", self.endpoint),
                        body,
                        headers: headers.clone(),
                        imp_ids,
                    });
                }
                Err(e) => errs.push(BidderError::BadInput(e.to_string())),
            }
        }

        // Banner request
        if !banner_imps.is_empty() {
            let mut req_copy = request.clone();
            req_copy.imp = banner_imps.clone();

            match serde_json::to_vec(&req_copy) {
                Ok(body) => {
                    let imp_ids = get_imp_ids(&banner_imps);
                    requests.push(RequestData {
                        method: "POST".to_string(),
                        uri: format!("{}/bid/banner", self.endpoint),
                        body,
                        headers: headers.clone(),
                        imp_ids,
                    });
                }
                Err(e) => errs.push(BidderError::BadInput(e.to_string())),
            }
        }

        // Fallback: if no video or banner imps, send single generic request
        if requests.is_empty() && errs.is_empty() {
            match serde_json::to_vec(request) {
                Ok(body) => {
                    requests.push(RequestData {
                        method: "POST".to_string(),
                        uri: self.endpoint.clone(),
                        body,
                        headers,
                        imp_ids: get_imp_ids(&request.imp),
                    });
                }
                Err(e) => errs.push(BidderError::BadInput(e.to_string())),
            }
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
