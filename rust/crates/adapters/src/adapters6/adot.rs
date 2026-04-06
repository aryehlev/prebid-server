use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;
use serde_json::Value;

pub struct AdotAdapter { pub endpoint: String }
impl AdotAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

#[derive(Deserialize, Default)]
struct AdotBidExt {
    adot: Option<BidExtAdot>,
}

#[derive(Deserialize, Default)]
struct BidExtAdot {
    #[serde(rename = "media_type", default)]
    media_type: String,
}

#[derive(Deserialize, Default)]
struct ExtImpBidder {
    bidder: Value,
}

#[derive(Deserialize, Default)]
struct ExtImpAdot {
    #[serde(rename = "publisherPath", default)]
    publisher_path: String,
}

fn get_adot_bid_type(bid: &openrtb::Bid) -> Result<BidType, BidderError> {
    if let Some(ext) = &bid.ext {
        if let Ok(adot_ext) = serde_json::from_value::<AdotBidExt>(ext.clone()) {
            if let Some(adot) = adot_ext.adot {
                return match adot.media_type.as_str() {
                    "banner" => Ok(BidType::Banner),
                    "video" => Ok(BidType::Video),
                    "native" => Ok(BidType::Native),
                    _ => Err(BidderError::BadServerResponse(
                        "unrecognized bid type in response from adot".to_string()
                    )),
                };
            }
        }
    }
    Err(BidderError::BadServerResponse(
        "unrecognized bid type in response from adot".to_string()
    ))
}

fn resolve_macros(bid: &mut openrtb::Bid) {
    let price = format!("{}", bid.price);
    if let Some(nurl) = &bid.nurl {
        bid.nurl = Some(nurl.replace("${AUCTION_PRICE}", &price));
    }
    if let Some(adm) = &bid.adm {
        bid.adm = Some(adm.replace("${AUCTION_PRICE}", &price));
    }
}

fn get_imp_adot_ext(imp: &openrtb::Imp) -> Option<ExtImpAdot> {
    let ext_val = imp.ext.as_ref()?;
    let bidder_ext: ExtImpBidder = serde_json::from_value(ext_val.clone()).ok()?;
    serde_json::from_value(bidder_ext.bidder).ok()
}

impl Bidder for AdotAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let body = match serde_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(format!("unable to marshal openrtb request ({})", e))]),
        };
        let publisher_path = request.imp.first()
            .and_then(get_imp_adot_ext)
            .map(|e| e.publisher_path)
            .unwrap_or_default();

        let uri = self.endpoint.replace("{PUBLISHER_PATH}", &publisher_path);

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        (vec![RequestData { method: "POST".to_string(), uri, body, headers, imp_ids: get_imp_ids(&request.imp) }], vec![])
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let cap = bid_resp.seatbid.first().map(|sb| sb.bid.len()).unwrap_or(1);
        let mut result = BidderResponse::with_capacity(cap);
        for sb in bid_resp.seatbid {
            for mut bid in sb.bid {
                if let Ok(bid_type) = get_adot_bid_type(&bid) {
                    resolve_macros(&mut bid);
                    result.bids.push(TypedBid::new(bid, bid_type));
                }
                // Skip bids with unrecognized type (matches Go behavior)
            }
        }
        Ok(result)
    }
}
