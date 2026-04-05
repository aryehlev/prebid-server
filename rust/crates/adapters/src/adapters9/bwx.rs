use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids, check_response_status};
use openrtb::BidResponse;
use openrtb_ext::BidType;

pub struct BwxAdapter { pub endpoint: String }
impl BwxAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

fn get_bid_type_from_mtype(mtype: u64, imp_id: &str) -> Result<BidType, BidderError> {
    match mtype {
        1 => Ok(BidType::Banner),
        2 => Ok(BidType::Video),
        4 => Ok(BidType::Native),
        _ => Err(BidderError::BadServerResponse(
            format!("failed to parse bid mtype ({}) for impression id \"{}\"", mtype, imp_id)
        )),
    }
}

impl Bidder for BwxAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();
        let mut errs = Vec::new();

        for imp in &request.imp {
            let bidder = imp.ext.as_ref()
                .and_then(|e| e.get("bidder"))
                .cloned()
                .unwrap_or(serde_json::Value::Null);

            let env = bidder.get("env").and_then(|v| v.as_str()).unwrap_or("");
            let pid = bidder.get("pid").and_then(|v| v.as_str()).unwrap_or("");

            if env.is_empty() || pid.is_empty() {
                errs.push(BidderError::BadInput(format!(
                    "Failed to deserialize BoldwinX extension: missing env or pid"
                )));
                continue;
            }

            let url = self.endpoint
                .replace("{{.Host}}", env)
                .replace("{{.SourceId}}", pid);

            let mut req_copy = request.clone();
            req_copy.imp = vec![imp.clone()];

            let imp_ids = get_imp_ids(&req_copy.imp);
            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };

            let mut headers = HashMap::new();
            headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
            headers.insert("Accept".to_string(), "application/json".to_string());

            requests.push(RequestData {
                method: "POST".to_string(),
                uri: url,
                body,
                headers,
                imp_ids,
            });
        }

        (requests, errs)
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        if bid_resp.seatbid.is_empty() {
            return Err(vec![BidderError::BadServerResponse("Array SeatBid cannot be empty".to_string())]);
        }

        let mut result = BidderResponse::with_capacity(5);

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let mtype = bid.mtype.unwrap_or(0) as u64;
                // Skip bids with unrecognized mtype (match Go behavior: continue on error).
                if let Ok(t) = get_bid_type_from_mtype(mtype, &bid.impid) {
                    result.bids.push(TypedBid::new(bid, t));
                }
            }
        }

        Ok(result)
    }
}
