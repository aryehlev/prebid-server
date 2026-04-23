use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;
use serde_json::Value;

pub struct BlisAdapter {
    pub endpoint: String,
}

impl BlisAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

#[derive(Deserialize)]
struct ExtImpBidder {
    bidder: Value,
}

#[derive(Deserialize)]
struct ImpExtBlis {
    #[serde(rename = "spid", default)]
    supply_partner_id: String,
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
    if let Some(burl) = &bid.burl {
        bid.burl = Some(burl.replace(PRICE_MACRO, &price));
    }
}

fn get_media_type_for_bid(bid: &openrtb::Bid) -> Result<BidType, BidderError> {
    match bid.mtype {
        Some(1) => Ok(BidType::Banner),
        Some(2) => Ok(BidType::Video),
        Some(4) => Ok(BidType::Native),
        _ => Err(BidderError::BadServerResponse(format!(
            "Failed to parse media type of impression ID \"{}\"", bid.impid
        ))),
    }
}

impl Bidder for BlisAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::BadInput(format!(
                "Invalid imp.ext for impression index 0. Error Infomation: missing ext"
            ))]);
        }

        let ext_val = match &request.imp[0].ext {
            Some(v) => v.clone(),
            None => return (vec![], vec![BidderError::BadInput(format!(
                "Invalid imp.ext for impression index 0. Error Infomation: missing ext"
            ))]),
        };

        let imp_ext: ExtImpBidder = match serde_json::from_value(ext_val) {
            Ok(v) => v,
            Err(e) => return (vec![], vec![BidderError::BadInput(format!(
                "Invalid imp.ext for impression index 0. Error Infomation: {}", e
            ))]),
        };

        let blis_ext: ImpExtBlis = match serde_json::from_value(imp_ext.bidder) {
            Ok(v) => v,
            Err(e) => return (vec![], vec![BidderError::BadInput(format!(
                "Invalid imp.ext.bidder for impression index 0. Error Infomation: {}", e
            ))]),
        };

        let url = self.endpoint.replace("{{.SupplyId}}", &blis_ext.supply_partner_id);

        let body = match serde_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let mut headers = HashMap::new();
        headers.insert("X-Supply-Partner-Id".to_string(), blis_ext.supply_partner_id);

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
