use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids, check_response_status};
use openrtb::BidResponse;
use openrtb_ext::BidType;

pub struct RoulaxAdapter { pub endpoint: String }
impl RoulaxAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

fn get_bid_type_from_mtype(mtype: u64, imp_id: &str) -> Result<BidType, BidderError> {
    match mtype {
        1 => Ok(BidType::Banner),
        2 => Ok(BidType::Video),
        4 => Ok(BidType::Native),
        _ => Err(BidderError::BadServerResponse(
            format!("Unable to fetch mediaType in impID: {}, mType: {}", imp_id, mtype)
        )),
    }
}

impl Bidder for RoulaxAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        // Get pid and publisherPath from first imp ext.bidder
        let first_imp = match request.imp.first() {
            Some(i) => i,
            None => return (vec![], vec![BidderError::BadInput("no impressions".to_string())]),
        };
        let bidder = first_imp.ext.as_ref()
            .and_then(|e| e.get("bidder"));
        let pid = bidder
            .and_then(|b| b.get("pid"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let publisher_path = bidder
            .and_then(|b| b.get("publisherPath"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // Build URL from template: replace {{.AccountID}} with pid, {{.PublisherID}} with publisherPath
        let url = self.endpoint
            .replace("{{.AccountID}}", &pid)
            .replace("{{.PublisherID}}", &publisher_path);

        let body = match serde_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());

        let imp_ids = get_imp_ids(&request.imp);
        (vec![RequestData {
            method: "POST".to_string(),
            uri: url,
            body,
            headers,
            imp_ids,
        }], vec![])
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(5);
        if let Some(cur) = &bid_resp.cur {
            if !cur.is_empty() { result.currency = cur.clone(); }
        }
        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let mtype = bid.ext.as_ref()
                    .and_then(|e| e.get("mtype"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                match get_bid_type_from_mtype(mtype, &bid.impid) {
                    Ok(t) => result.bids.push(TypedBid::new(bid, t)),
                    Err(e) => errs.push(e),
                }
            }
        }
        if !errs.is_empty() { return Err(errs); }
        Ok(result)
    }
}
