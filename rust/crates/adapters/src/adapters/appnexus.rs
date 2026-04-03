use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_bid_type_from_imp, get_imp_ids};
use openrtb_ext::BidType;
use serde::{Deserialize, Serialize};

const MAX_IMPS_PER_REQ: usize = 10;

pub struct AppnexusAdapter {
    pub endpoint: String,
}

impl AppnexusAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

#[derive(Serialize, Deserialize, Default)]
struct ExtImpAppnexus {
    #[serde(rename = "member", default)]
    pub member: String,
    #[serde(rename = "placement_id", default)]
    pub placement_id: i64,
    #[serde(rename = "inv_code", default)]
    pub inv_code: String,
}

#[derive(Deserialize, Default)]
struct ImpExt {
    #[serde(default)]
    bidder: ExtImpAppnexus,
}

impl Bidder for AppnexusAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut valid_imps = Vec::new();
        let mut unique_member_id = String::new();

        for imp in &request.imp {
            let imp_ext: ImpExt = match imp.ext.as_ref().and_then(|e| serde_json::from_value(e.clone()).ok()) {
                Some(e) => e,
                None => {
                    errs.push(BidderError::BadInput("Failed to parse appnexus imp ext".to_string()));
                    continue;
                }
            };

            let member_id = imp_ext.bidder.member.clone();
            if !member_id.is_empty() {
                if unique_member_id.is_empty() {
                    unique_member_id = member_id;
                } else if unique_member_id != member_id {
                    errs.push(BidderError::BadInput(format!(
                        "all request.imp[i].ext.prebid.bidder.appnexus.member params must match. Request contained member IDs {} and {}",
                        unique_member_id, member_id
                    )));
                    return (vec![], errs);
                }
            }

            valid_imps.push(imp.clone());
        }

        if valid_imps.is_empty() {
            return (vec![], errs);
        }

        // Build endpoint URI with member_id if present
        let uri = if !unique_member_id.is_empty() {
            if self.endpoint.contains('?') {
                format!("{}&member_id={}", self.endpoint, unique_member_id)
            } else {
                format!("{}?member_id={}", self.endpoint, unique_member_id)
            }
        } else {
            self.endpoint.clone()
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        let mut requests = Vec::new();

        // Split into batches of max MAX_IMPS_PER_REQ
        for chunk in valid_imps.chunks(MAX_IMPS_PER_REQ) {
            let mut req_copy = request.clone();
            req_copy.imp = chunk.to_vec();

            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(BidderError::BadInput(e.to_string()));
                    return (vec![], errs);
                }
            };

            let imp_ids = get_imp_ids(&req_copy.imp);

            requests.push(RequestData {
                method: "POST".to_string(),
                uri: uri.clone(),
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
