use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct AdtelligentAdapter {
    pub endpoint: String,
}

impl AdtelligentAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

impl Bidder for AdtelligentAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        // Group imps by source_id from ext.bidder.source_id
        let mut source_to_imps: std::collections::HashMap<i64, Vec<openrtb::Imp>> = std::collections::HashMap::new();

        for imp in &request.imp {
            if imp.banner.is_none() && imp.video.is_none() {
                errs.push(BidderError::BadInput(format!(
                    "ignoring imp id={}, Adtelligent supports only Video and Banner", imp.id
                )));
                continue;
            }

            let source_id = imp.ext.as_ref()
                .and_then(|e| e.get("bidder"))
                .and_then(|b| b.get("source_id").or_else(|| b.get("sourceId")))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);

            source_to_imps.entry(source_id).or_default().push(imp.clone());
        }

        if source_to_imps.is_empty() {
            return (vec![], errs);
        }

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        let mut requests = Vec::new();
        for (source_id, imps) in source_to_imps {
            let mut req = request.clone();
            req.imp = imps;
            let body = match serde_json::to_vec(&req) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(BidderError::BadInput(format!("error encoding bidRequest: {}", e)));
                    continue;
                }
            };
            let uri = format!("{}?aid={}", self.endpoint, source_id);
            let imp_ids = get_imp_ids(&req.imp);
            requests.push(RequestData {
                method: "POST".to_string(),
                uri,
                body,
                headers: headers.clone(),
                imp_ids,
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
        if response.status_code == 204 {
            return Ok(BidderResponse::new());
        }
        if let Err(e) = crate::check_response_status(response.status_code) {
            return Err(vec![e]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(format!("error decoding response: {}", e))])?;

        let mut result = BidderResponse::new();
        let mut errs = Vec::new();

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let found = internal.imp.iter().find(|i| i.id == bid.impid);
                match found {
                    Some(imp) => {
                        let bid_type = if imp.video.is_some() {
                            BidType::Video
                        } else {
                            BidType::Banner
                        };
                        result.bids.push(TypedBid::new(bid, bid_type));
                    }
                    None => {
                        errs.push(BidderError::BadServerResponse(format!(
                            "ignoring bid id={}, request doesn't contain any impression with id={}",
                            bid.id, bid.impid
                        )));
                    }
                }
            }
        }

        Ok(result)
    }
}
