use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde_json::Value;

pub struct BidtheatreAdapter {
    pub endpoint: String,
}

impl BidtheatreAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

const PRICE_MACRO: &str = "${AUCTION_PRICE}";

fn resolve_macros(bid: &mut openrtb::Bid) {
    let price = format!("{}", bid.price);
    if let Some(nurl) = &bid.nurl {
        bid.nurl = Some(nurl.replace(PRICE_MACRO, &price));
    }
    if let Some(adm) = &bid.adm {
        bid.adm = Some(adm.replace(PRICE_MACRO, &price));
    }
}

fn get_media_type_for_bid(bid: &openrtb::Bid) -> Result<BidType, BidderError> {
    if let Some(ext) = &bid.ext {
        if let Ok(obj) = serde_json::from_value::<serde_json::Map<String, Value>>(ext.clone()) {
            if let Some(prebid) = obj.get("prebid") {
                if let Some(bid_type_str) = prebid.get("type").and_then(|v| v.as_str()) {
                    return match bid_type_str {
                        "banner" => Ok(BidType::Banner),
                        "video" => Ok(BidType::Video),
                        "native" => Ok(BidType::Native),
                        "audio" => Ok(BidType::Audio),
                        _ => Err(BidderError::BadServerResponse(format!(
                            "Failed to parse impression \"{}\" mediatype", bid.impid
                        ))),
                    };
                }
            }
        }
    }
    Err(BidderError::BadServerResponse(format!(
        "Failed to parse impression \"{}\" mediatype", bid.impid
    )))
}

impl Bidder for BidtheatreAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let body = match serde_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        (vec![RequestData {
            method: "POST".to_string(),
            uri: self.endpoint.clone(),
            body,
            headers: HashMap::new(),
            imp_ids: get_imp_ids(&request.imp),
        }], vec![])
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) {
            return Err(vec![e]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(internal.imp.len());
        if let Some(cur) = &bid_resp.cur {
            result.currency = cur.clone();
        }
        let mut errs = Vec::new();

        for sb in bid_resp.seatbid {
            for mut bid in sb.bid {
                resolve_macros(&mut bid);
                match get_media_type_for_bid(&bid) {
                    Ok(bt) => result.bids.push(TypedBid::new(bid, bt)),
                    Err(e) => errs.push(e),
                }
            }
        }

        Ok(result)
    }
}
