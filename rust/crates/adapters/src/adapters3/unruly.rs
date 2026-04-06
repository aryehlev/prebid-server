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

fn get_media_type_for_imp(imp_id: &str, imps: &[openrtb::Imp]) -> Result<BidType, Vec<BidderError>> {
    for imp in imps {
        if imp.id == imp_id {
            if imp.banner.is_some() {
                return Ok(BidType::Banner);
            } else if imp.video.is_some() {
                return Ok(BidType::Video);
            } else {
                return Err(vec![BidderError::BadServerResponse(
                    "bid responses mediaType didn't match supported mediaTypes".to_string(),
                )]);
            }
        }
    }
    Err(vec![BidderError::BadServerResponse(format!(
        "Bid response imp ID {} not found in bid request",
        imp_id
    ))])
}

impl Bidder for UnrulyAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();

        // Validate each imp has required bidder ext with siteId
        for imp in &request.imp {
            let bidder_ext = imp.ext.as_ref().and_then(|e| e.get("bidder"));
            if bidder_ext.is_none() {
                errs.push(BidderError::BadInput(format!(
                    "ext data not provided in imp id={}. Abort all Request",
                    imp.id
                )));
                return (vec![], errs);
            }

            let site_id = bidder_ext
                .and_then(|b| b.get("siteId").or_else(|| b.get("siteid")));

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
        if response.status_code == 400 {
            return Err(vec![BidderError::BadInput(format!(
                "unexpected status code: {}. Run with request.debug = 1 for more info",
                response.status_code
            ))]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "unexpected status code: {}. Run with request.debug = 1 for more info",
                response.status_code
            ))]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(format!("bad server response: {}. ", e))])?;

        let mut result = BidderResponse::with_capacity(internal.imp.len());
        let mut errs = Vec::new();

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                match get_media_type_for_imp(&bid.impid, &internal.imp) {
                    Ok(bid_type) => {
                        let mut typed_bid = TypedBid::new(bid.clone(), bid_type.clone());
                        // Include video duration if available (dur is a field on Bid, not in ext)
                        if bid_type == BidType::Video {
                            if let Some(dur) = bid.dur {
                                if dur > 0.0 {
                                    typed_bid.bid_video = Some(openrtb_ext::ExtBidPrebidVideo {
                                        duration: dur as i32,
                                        ..Default::default()
                                    });
                                }
                            }
                        }
                        result.bids.push(typed_bid);
                    }
                    Err(mut e) => errs.append(&mut e),
                }
            }
        }

        Ok(result)
    }
}
