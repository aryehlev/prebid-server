use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use openrtb_ext::BidType;
use serde::Deserialize;
use serde_json::Value;

pub struct AsoAdapter {
    pub endpoint: String,
}

impl AsoAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

#[derive(Deserialize)]
struct ExtImpBidder { bidder: Value }

#[derive(Deserialize)]
struct ExtImpAso {
    #[serde(default)]
    zone: i32,
}

#[derive(Deserialize)]
struct ExtBidPrebid {
    #[serde(rename = "type", default)]
    bid_type: String,
}

#[derive(Deserialize)]
struct ExtBid {
    prebid: Option<ExtBidPrebid>,
}

const PRICE_MACRO: &str = "${AUCTION_PRICE}";

fn resolve_macros_bid(bid: &mut openrtb::Bid) {
    let price = format!("{}", bid.price);
    if let Some(nurl) = &bid.nurl {
        bid.nurl = Some(nurl.replace(PRICE_MACRO, &price));
    }
    if let Some(adm) = &bid.adm {
        bid.adm = Some(adm.replace(PRICE_MACRO, &price));
    }
}

fn get_media_type_from_bid(bid: &openrtb::Bid) -> Result<BidType, BidderError> {
    if let Some(ext) = &bid.ext {
        if let Ok(bid_ext) = serde_json::from_value::<ExtBid>(ext.clone()) {
            if let Some(prebid) = bid_ext.prebid {
                return match prebid.bid_type.as_str() {
                    "banner" => Ok(BidType::Banner),
                    "video"  => Ok(BidType::Video),
                    "audio"  => Ok(BidType::Audio),
                    "native" => Ok(BidType::Native),
                    _ => Err(BidderError::BadServerResponse(
                        format!("Failed to get type of bid \"{}\"", bid.impid)
                    )),
                };
            }
        }
    }
    Err(BidderError::BadServerResponse(format!("Failed to get type of bid \"{}\"", bid.impid)))
}

impl Bidder for AsoAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();
        let mut errs = Vec::new();

        for imp in &request.imp {
            let ext_val = match &imp.ext {
                Some(v) => v.clone(),
                None => { errs.push(BidderError::BadInput(format!("invalid imp.ext, missing"))); continue; }
            };

            let bidder_ext: ExtImpBidder = match serde_json::from_value(ext_val) {
                Ok(v) => v,
                Err(e) => { errs.push(BidderError::BadInput(format!("invalid imp.ext, {}", e))); continue; }
            };

            let imp_ext: ExtImpAso = match serde_json::from_value(bidder_ext.bidder) {
                Ok(v) => v,
                Err(e) => { errs.push(BidderError::BadInput(format!("invalid bidderExt.Bidder, {}", e))); continue; }
            };

            // Build endpoint URL with zone param
            let url = self.endpoint.replace("{{.ZoneID}}", &imp_ext.zone.to_string());

            let mut req = request.clone();
            req.imp = vec![imp.clone()];

            let body = match serde_json::to_vec(&req) {
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
                imp_ids: vec![imp.id.clone()],
            });
        }

        (requests, errs)
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(internal.imp.len());
        if let Some(cur) = &bid_resp.cur {
            result.currency = cur.clone();
        }
        let mut errs = Vec::new();

        for sb in bid_resp.seatbid {
            for mut bid in sb.bid {
                resolve_macros_bid(&mut bid);
                match get_media_type_from_bid(&bid) {
                    Ok(bid_type) => result.bids.push(TypedBid::new(bid, bid_type)),
                    Err(e) => errs.push(e),
                }
            }
        }

        Ok(result)
    }
}
