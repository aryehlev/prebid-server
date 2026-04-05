use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids, check_response_status};
use openrtb_ext::BidType;
use serde::Deserialize;

pub struct TradplusAdapter { pub endpoint: String }
impl TradplusAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

#[derive(Debug, Deserialize)]
struct ExtImpTradPlus {
    #[serde(rename = "accountID", default)]
    account_id: String,
    #[serde(rename = "zoneID", default)]
    zone_id: String,
}

fn get_media_type_for_bid(mtype: i32, imp_id: &str) -> Result<BidType, BidderError> {
    match mtype {
        1 => Ok(BidType::Banner),
        2 => Ok(BidType::Video),
        4 => Ok(BidType::Native),
        _ => Err(BidderError::BadServerResponse(format!(
            "unrecognized bid type in response from tradplus for bid {}", imp_id
        ))),
    }
}

impl Bidder for TradplusAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::BadInput("no impressions".to_string())]);
        }

        let bidder_val = match request.imp[0].ext.as_ref().and_then(|e| e.get("bidder")) {
            Some(v) => v.clone(),
            None => return (vec![], vec![BidderError::BadInput("Error parsing tradplusExt - missing bidder ext".to_string())]),
        };
        let tradplus_ext: ExtImpTradPlus = match serde_json::from_value(bidder_val) {
            Ok(e) => e,
            Err(e) => return (vec![], vec![BidderError::BadInput(format!("Error parsing bidderExt - {}", e))]),
        };

        let url = self.endpoint
            .replace("{{.AccountID}}", &tradplus_ext.account_id)
            .replace("{{.ZoneID}}", &tradplus_ext.zone_id);

        let body = match serde_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());

        (vec![RequestData {
            method: "POST".to_string(),
            uri: url,
            body,
            headers,
            imp_ids: get_imp_ids(&request.imp),
        }], vec![])
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = check_response_status(response.status_code) {
            return Err(vec![BidderError::BadInput(format!("Unexpected status code: {}.", response.status_code))]);
        }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(internal.imp.len());
        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let mtype = bid.mtype.unwrap_or(0);
                match get_media_type_for_bid(mtype, &bid.impid) {
                    Ok(t) => result.bids.push(TypedBid::new(bid, t)),
                    Err(e) => errs.push(e),
                }
            }
        }
        Ok(result)
    }
}
