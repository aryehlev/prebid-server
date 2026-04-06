use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;
use serde_json::Value;

pub struct AdtonosAdapter { pub endpoint: String }
impl AdtonosAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

#[derive(Deserialize)]
struct ExtImpBidder { bidder: Value }

#[derive(Deserialize)]
struct ImpExtAdTonos {
    #[serde(rename = "supplierId")]
    supplier_id: String,
}

fn get_adtonos_bid_type(bid: &openrtb::Bid, imps: &[openrtb::Imp]) -> Result<BidType, BidderError> {
    if let Some(mtype) = bid.mtype {
        return match mtype {
            3 => Ok(BidType::Audio),
            2 => Ok(BidType::Video),
            1 => Ok(BidType::Banner),
            4 => Ok(BidType::Native),
            _ => Err(BidderError::BadInput(format!("Unsupported bidtype for bid: \"{}\"", bid.impid))),
        };
    }
    for imp in imps {
        if imp.id == bid.impid {
            if imp.audio.is_some() { return Ok(BidType::Audio); }
            if imp.video.is_some() { return Ok(BidType::Video); }
            return Err(BidderError::BadInput(format!("Unsupported bidtype for bid: \"{}\"", bid.impid)));
        }
    }
    Err(BidderError::BadInput(format!("Failed to find impression: \"{}\"", bid.impid)))
}

impl Bidder for AdtonosAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::BadInput("no impressions".to_string())]);
        }
        let imp = &request.imp[0];
        let bidder_ext: ExtImpBidder = match imp.ext.as_ref()
            .and_then(|e| serde_json::from_value(e.clone()).ok()) {
            Some(v) => v,
            None => return (vec![], vec![BidderError::BadInput(
                format!("Invalid imp.ext for impression index 0")
            )]),
        };
        let imp_ext: ImpExtAdTonos = match serde_json::from_value(bidder_ext.bidder) {
            Ok(v) => v,
            Err(e) => return (vec![], vec![BidderError::BadInput(
                format!("Invalid imp.ext.bidder for impression index 0. Error Infomation: {}", e)
            )]),
        };
        let uri = self.endpoint.replace("{{.PublisherID}}", &imp_ext.supplier_id);
        let body = match serde_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
        (vec![RequestData { method: "POST".to_string(), uri, body, headers, imp_ids: get_imp_ids(&request.imp) }], vec![])
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(internal.imp.len());
        if let Some(cur) = &bid_resp.cur { if !cur.is_empty() { result.currency = cur.clone(); } }
        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                match get_adtonos_bid_type(&bid, &internal.imp) {
                    Ok(bt) => result.bids.push(TypedBid::new(bid, bt)),
                    Err(e) => errs.push(e),
                }
            }
        }
        // Return bids with errors accumulated (errors don't prevent returning bids)
        if !errs.is_empty() && result.bids.is_empty() {
            return Err(errs);
        }
        Ok(result)
    }
}
