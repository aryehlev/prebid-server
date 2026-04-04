use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct AdmanAdapter {
    pub endpoint: String,
}

impl AdmanAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

fn get_media_type_for_imp(imp_id: &str, imps: &[openrtb::Imp]) -> Result<BidType, BidderError> {
    for imp in imps {
        if imp.id == imp_id {
            let bid_type = if imp.banner.is_none() && imp.video.is_some() {
                BidType::Video
            } else {
                BidType::Banner
            };
            return Ok(bid_type);
        }
    }
    Err(BidderError::BadInput(format!("Failed to find impression \"{}\" ", imp_id)))
}

impl Bidder for AdmanAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();
        let mut errs = Vec::new();

        // One request per imp, setting tagid from imp.ext.bidder.TagID
        for imp in &request.imp {
            // Go: ExtImpAdman{TagID string `json:"TagID"`} from imp.ext.bidder
            let tag_id = imp.ext.as_ref()
                .and_then(|e| e.get("bidder"))
                .and_then(|b| b.get("TagID"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let mut req = request.clone();
            let mut imp_copy = imp.clone();
            if let Some(tid) = tag_id {
                imp_copy.tagid = Some(tid);
            }
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
        internal: &openrtb::BidRequest,
        _external: &RequestData,
        response: &ResponseData,
    ) -> Result<BidderResponse, Vec<BidderError>> {
        // Go returns nil, nil for 204
        if response.status_code == 204 {
            return Ok(BidderResponse::new());
        }
        // Go returns BadServerResponse for 404
        if response.status_code == 404 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info",
                response.status_code
            ))]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(1);
        let mut errs = Vec::new();

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                match get_media_type_for_imp(&bid.impid, &internal.imp) {
                    Ok(bid_type) => result.bids.push(TypedBid::new(bid, bid_type)),
                    Err(e) => errs.push(e),
                }
            }
        }

        // Per-bid media-type lookup errors are non-fatal. The trait signature
        // (Result<BidderResponse, Vec<BidderError>>) does not allow returning both
        // successful bids and non-fatal errors, so errors are dropped here.
        let _ = errs;
        Ok(result)
    }
}
