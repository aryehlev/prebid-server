use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;

pub struct DatablocksAdapter { pub endpoint: String }
impl DatablocksAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

#[derive(Debug, Default, Deserialize, PartialEq, Eq, Hash, Clone)]
struct ExtImpDatablocks {
    #[serde(rename = "sourceId", default)]
    source_id: i64,
}

fn get_bidder_params(imp: &openrtb::Imp) -> Result<ExtImpDatablocks, BidderError> {
    let bidder_val = imp.ext.as_ref()
        .and_then(|e| e.get("bidder"))
        .cloned()
        .ok_or_else(|| BidderError::BadInput(format!("Missing bidder ext: no ext on imp")))?;
    let ext: ExtImpDatablocks = serde_json::from_value(bidder_val)
        .map_err(|e| BidderError::BadInput(format!("Cannot Resolve sourceId: {}", e)))?;
    if ext.source_id < 1 {
        return Err(BidderError::BadInput("Invalid/Missing SourceId".to_string()));
    }
    Ok(ext)
}

fn get_media_type(imp_id: &str, imps: &[openrtb::Imp]) -> BidType {
    for imp in imps {
        if imp.id == imp_id {
            if imp.video.is_some() {
                return BidType::Video;
            } else if imp.native.is_some() {
                return BidType::Native;
            } else {
                return BidType::Banner;
            }
        }
    }
    BidType::Banner
}

impl Bidder for DatablocksAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut imp_groups: HashMap<i64, Vec<openrtb::Imp>> = HashMap::new();

        for imp in &request.imp {
            match get_bidder_params(imp) {
                Ok(params) => {
                    imp_groups.entry(params.source_id).or_default().push(imp.clone());
                }
                Err(e) => {
                    errs.push(e);
                }
            }
        }

        let mut requests = Vec::new();

        for (source_id, imps) in imp_groups {
            let mut req = request.clone();
            req.imp = imps;

            let body = match serde_json::to_vec(&req) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(BidderError::BadInput(e.to_string()));
                    continue;
                }
            };

            // Build URL: replace {{.SourceId}} macro
            let url = self.endpoint.replace("{{.SourceId}}", &source_id.to_string());

            let mut headers = HashMap::new();
            headers.insert("Content-Type".to_string(), "application/json".to_string());
            headers.insert("Accept".to_string(), "application/json".to_string());

            let imp_ids = get_imp_ids(&req.imp);
            requests.push(RequestData { method: "POST".to_string(), uri: url, body, headers, imp_ids });
        }

        (requests, errs)
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "ERR, response with status {}", response.status_code
            ))]);
        }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(5);
        if let Some(cur) = bid_resp.cur {
            result.currency = cur;
        }
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = get_media_type(&bid.impid, &internal.imp);
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        Ok(result)
    }
}
