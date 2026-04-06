use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;
use serde_json::Value;

pub struct AxonixAdapter { pub endpoint: String }
impl AxonixAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

#[derive(Deserialize)]
struct ExtImpBidder { bidder: Value }

#[derive(Deserialize)]
struct ExtImpAxonix {
    #[serde(rename = "supplyId", default)]
    supply_id: String,
}

const PRICE_MACRO: &str = "${AUCTION_PRICE}";

fn resolve_macros_bid(bid: &mut openrtb::Bid) {
    // Format price with no trailing zeros, matching Go's strconv.FormatFloat(price, 'f', -1, 64)
    let price = format_price(bid.price);
    if let Some(nurl) = &bid.nurl {
        bid.nurl = Some(nurl.replace(PRICE_MACRO, &price));
    }
    if let Some(adm) = &bid.adm {
        bid.adm = Some(adm.replace(PRICE_MACRO, &price));
    }
}

fn format_price(price: f64) -> String {
    // Match Go's strconv.FormatFloat(price, 'f', -1, 64): decimal notation, minimum digits
    let s = format!("{:.10}", price);
    let s = s.trim_end_matches('0');
    let s = s.trim_end_matches('.');
    if s.is_empty() || s == "-" {
        return "0".to_string();
    }
    s.to_string()
}

fn get_media_type(imp_id: &str, imps: &[openrtb::Imp]) -> BidType {
    for imp in imps {
        if imp.id == imp_id {
            if imp.native.is_some() { return BidType::Native; }
            if imp.video.is_some()  { return BidType::Video;  }
            return BidType::Banner;
        }
    }
    BidType::Banner
}

impl Bidder for AxonixAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![]);
        }

        let ext_val = match &request.imp[0].ext {
            Some(v) => v.clone(),
            None => return (vec![], vec![BidderError::BadInput("missing ext".to_string())]),
        };

        let bidder_ext: ExtImpBidder = match serde_json::from_value(ext_val) {
            Ok(v) => v,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let axonix_ext: ExtImpAxonix = match serde_json::from_value(bidder_ext.bidder) {
            Ok(v) => v,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let url = self.endpoint.replace("{{.AccountID}}", &urlencoded(&axonix_ext.supply_id));

        let body = match serde_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json".to_string());

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
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!("Unexpected status code: {}.", response.status_code))]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(internal.imp.len());
        if let Some(cur) = &bid_resp.cur {
            result.currency = cur.clone();
        }

        for sb in bid_resp.seatbid {
            for mut bid in sb.bid {
                resolve_macros_bid(&mut bid);
                let bid_type = get_media_type(&bid.impid, &internal.imp);
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }

        Ok(result)
    }
}

fn urlencoded(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9'
            | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => { out.push('%'); out.push_str(&format!("{:02X}", b)); }
        }
    }
    out
}
