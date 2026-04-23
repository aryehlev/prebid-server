use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct UnrulyAdapter {
    pub endpoint: String,
}

impl UnrulyAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

fn get_media_type_for_imp(imp_id: &str, imps: &[openrtb::Imp]) -> Result<BidType, BidderError> {
    for imp in imps {
        if imp.id == imp_id {
            if imp.banner.is_some() {
                return Ok(BidType::Banner);
            } else if imp.video.is_some() {
                return Ok(BidType::Video);
            } else {
                return Err(BidderError::BadServerResponse(
                    "bid responses mediaType didn't match supported mediaTypes".to_string(),
                ));
            }
        }
    }
    Err(BidderError::BadServerResponse(format!(
        "Bid response imp ID {} not found in bid request",
        imp_id
    )))
}

impl Bidder for UnrulyAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();

        // Validate each imp has required bidder ext (siteId)
        for imp in &request.imp {
            let has_bidder_ext = imp
                .ext
                .as_ref()
                .and_then(|e| e.get("bidder"))
                .is_some();

            if !has_bidder_ext {
                errs.push(BidderError::BadInput(format!(
                    "ext data not provided in imp id={}. Abort all Request",
                    imp.id
                )));
                return (vec![], errs);
            }

            // Check for siteId (Unruly requires it)
            let site_id = imp
                .ext
                .as_ref()
                .and_then(|e| e.get("bidder"))
                .and_then(|b| b.get("siteId"))
                .or_else(|| {
                    imp.ext
                        .as_ref()
                        .and_then(|e| e.get("bidder"))
                        .and_then(|b| b.get("siteid"))
                });

            if site_id.is_none() {
                errs.push(BidderError::BadInput(format!(
                    "siteid not provided in imp id={}. Abort all Request",
                    imp.id
                )));
                return (vec![], errs);
            }
        }

        if self.endpoint.is_empty() {
            return (vec![], errs);
        }

        let body = match serde_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => {
                errs.push(BidderError::BadInput(e.to_string()));
                return (vec![], errs);
            }
        };

        let mut headers = HashMap::new();
        headers.insert(
            "Content-Type".to_string(),
            "application/json;charset=utf-8".to_string(),
        );
        headers.insert("Accept".to_string(), "application/json".to_string());

        (
            vec![RequestData {
                method: "POST".to_string(),
                uri: self.endpoint.clone(),
                body,
                headers,
                imp_ids: get_imp_ids(&request.imp),
            }],
            errs,
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
        if let Err(e) = crate::check_response_status(response.status_code) {
            return Err(vec![e]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(internal.imp.len());
        let mut errs = Vec::new();

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = match get_media_type_for_imp(&bid.impid, &internal.imp) {
                    Ok(t) => t,
                    Err(e) => {
                        errs.push(e);
                        continue;
                    }
                };

                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }

        if errs.is_empty() {
            Ok(result)
        } else {
            // Return partial results like Go behavior
            Ok(result)
        }
    }
}
