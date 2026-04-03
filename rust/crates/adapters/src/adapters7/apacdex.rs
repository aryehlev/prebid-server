use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;
use serde_json::Value;

pub struct ApacdexAdapter {
    pub endpoint: String,
}

impl ApacdexAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

#[derive(Deserialize)]
struct ExtImpBidder { bidder: Value }

#[derive(Deserialize)]
struct ExtBidPrebid {
    #[serde(rename = "type", default)]
    bid_type: String,
}

#[derive(Deserialize)]
struct ExtBid {
    prebid: Option<ExtBidPrebid>,
}

fn preprocess(request: &mut openrtb::BidRequest) -> Result<(), BidderError> {
    if request.imp.is_empty() {
        return Err(BidderError::BadInput("No Imps in Bid Request".to_string()));
    }

    for imp in &mut request.imp {
        let ext_val = imp.ext.as_ref()
            .ok_or_else(|| BidderError::BadInput("missing imp ext".to_string()))?
            .clone();

        let bidder_ext: ExtImpBidder = serde_json::from_value(ext_val)
            .map_err(|e| BidderError::BadInput(e.to_string()))?;

        // Validate it can be parsed (apacdex specific ext)
        let _: Value = bidder_ext.bidder.clone();

        // Replace imp.ext with just the bidder-specific portion
        imp.ext = Some(bidder_ext.bidder);
    }

    Ok(())
}

impl Bidder for ApacdexAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut req = request.clone();

        if let Err(e) = preprocess(&mut req) {
            return (vec![], vec![e]);
        }

        let body = match serde_json::to_vec(&req) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
        headers.insert("x-openrtb-version".to_string(), "2.5".to_string());

        (vec![RequestData {
            method: "POST".to_string(),
            uri: self.endpoint.clone(),
            body,
            headers,
            imp_ids: get_imp_ids(&request.imp),
        }], vec![])
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code == 400 {
            return Err(vec![BidderError::BadInput(
                "Unexpected status code: 400. Bad request from publisher. Run with request.debug = 1 for more info.".to_string()
            )]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(
                format!("Unexpected status code: {}. Run with request.debug = 1 for more info.", response.status_code)
            )]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        if bid_resp.seatbid.is_empty() {
            return Ok(BidderResponse::new());
        }

        let mut result = BidderResponse::with_capacity(internal.imp.len());
        if let Some(cur) = &bid_resp.cur {
            result.currency = cur.clone();
        }
        let mut errs = Vec::new();

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                match get_bid_type_from_ext(&bid) {
                    Ok(bid_type) => result.bids.push(TypedBid::new(bid, bid_type)),
                    Err(e) => errs.push(e),
                }
            }
        }

        Ok(result)
    }
}

fn get_bid_type_from_ext(bid: &openrtb::Bid) -> Result<BidType, BidderError> {
    if let Some(ext) = &bid.ext {
        if let Ok(bid_ext) = serde_json::from_value::<ExtBid>(ext.clone()) {
            if let Some(prebid) = bid_ext.prebid {
                return match prebid.bid_type.as_str() {
                    "banner" => Ok(BidType::Banner),
                    "video"  => Ok(BidType::Video),
                    "audio"  => Ok(BidType::Audio),
                    "native" => Ok(BidType::Native),
                    _ => Err(BidderError::BadServerResponse(
                        format!("Failed to parse bid mediatype for impression \"{}\"", bid.impid)
                    )),
                };
            }
        }
    }

    Err(BidderError::BadServerResponse(
        format!("Failed to parse bid mediatype for impression \"{}\"", bid.impid)
    ))
}
