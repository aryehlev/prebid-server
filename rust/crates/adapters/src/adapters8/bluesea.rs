use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;
use serde_json::Value;

pub struct BlueseaAdapter { pub endpoint: String }
impl BlueseaAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

#[derive(Deserialize)]
struct ExtImpBidder { bidder: Value }

#[derive(Deserialize)]
struct ExtImpBluesea {
    #[serde(default)]
    pubid: String,
    #[serde(default)]
    token: String,
}

#[derive(Deserialize)]
struct BlueseaBidExt {
    #[serde(default)]
    mediatype: String,
}

fn get_media_type_for_bid(bid: &openrtb::Bid) -> Result<BidType, BidderError> {
    let ext = bid.ext.as_ref()
        .ok_or_else(|| BidderError::BadServerResponse("Error in parsing bid.ext".to_string()))?;
    let bid_ext: BlueseaBidExt = serde_json::from_value(ext.clone())
        .map_err(|_| BidderError::BadServerResponse("Error in parsing bid.ext".to_string()))?;
    match bid_ext.mediatype.as_str() {
        "banner" => Ok(BidType::Banner),
        "native" => Ok(BidType::Native),
        "video"  => Ok(BidType::Video),
        other    => Err(BidderError::BadServerResponse(format!("Unknown bid type, {}", other))),
    }
}

impl Bidder for BlueseaAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::BadInput("Empty Imp objects".to_string())]);
        }

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        let mut results = Vec::new();
        let mut errs = Vec::new();

        for imp in &request.imp {
            let ext_val = match &imp.ext {
                Some(v) => v.clone(),
                None => {
                    errs.push(BidderError::BadInput(format!(
                        "Error in parsing imp.ext. err = missing ext, imp.ext = "
                    )));
                    continue;
                }
            };

            let bidder_ext: ExtImpBidder = match serde_json::from_value(ext_val) {
                Ok(v) => v,
                Err(e) => {
                    errs.push(BidderError::BadInput(format!(
                        "Error in parsing imp.ext. err = {}, imp.ext = {}",
                        e,
                        imp.ext.as_ref().map(|v| v.to_string()).unwrap_or_default()
                    )));
                    continue;
                }
            };

            let bluesea_ext: ExtImpBluesea = match serde_json::from_value(bidder_ext.bidder.clone()) {
                Ok(v) => v,
                Err(e) => {
                    errs.push(BidderError::BadInput(format!(
                        "Error in parsing imp.ext.bidder. err = {}, bidder = {}",
                        e, bidder_ext.bidder
                    )));
                    continue;
                }
            };

            if bluesea_ext.pubid.is_empty() || bluesea_ext.token.is_empty() {
                errs.push(BidderError::BadInput(
                    "Error in parsing imp.ext.bidder, empty pubid or token".to_string()
                ));
                continue;
            }

            let body = match serde_json::to_vec(request) {
                Ok(b) => b,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };

            let url = format!("{}?pubid={}&token={}", self.endpoint, bluesea_ext.pubid, bluesea_ext.token);

            results.push(RequestData {
                method: "POST".to_string(),
                uri: url,
                body,
                headers: headers.clone(),
                imp_ids: get_imp_ids(&request.imp),
            });
        }

        if results.is_empty() && errs.is_empty() {
            errs.push(BidderError::BadInput("Empty RequestData".to_string()));
        }

        (results, errs)
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|_| vec![BidderError::BadServerResponse("Error in parsing bidresponse body".to_string())])?;

        let mut result = BidderResponse::with_capacity(1);
        if let Some(cur) = &bid_resp.cur {
            if !cur.is_empty() { result.currency = cur.clone(); }
        }
        let mut errs = Vec::new();

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                match get_media_type_for_bid(&bid) {
                    Ok(bid_type) => result.bids.push(TypedBid::new(bid, bid_type)),
                    Err(e) => errs.push(e),
                }
            }
        }

        Ok(result)
    }
}
