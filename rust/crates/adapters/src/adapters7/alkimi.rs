use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids, get_bid_type_from_imp};
use openrtb_ext::BidType;
use serde_json::Value;

pub struct AlkimiAdapter {
    pub endpoint: String,
}

impl AlkimiAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
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

fn update_imps(imps: &[openrtb::Imp]) -> (Vec<openrtb::Imp>, Vec<BidderError>) {
    let mut updated = Vec::with_capacity(imps.len());
    let mut errs = Vec::new();

    for imp in imps {
        let ext_val = match &imp.ext {
            Some(v) => v.clone(),
            None => { errs.push(BidderError::BadInput(format!("missing ext for imp {}", imp.id))); continue; }
        };

        let bidder_ext: std::collections::HashMap<String, Value> = match serde_json::from_value(ext_val) {
            Ok(m) => m,
            Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
        };

        let bidder_val = match bidder_ext.get("bidder") {
            Some(v) => v.clone(),
            None => { errs.push(BidderError::BadInput("missing bidder key in ext".to_string())); continue; }
        };

        #[derive(serde::Deserialize, serde::Serialize, Clone)]
        struct ExtImpAlkimi {
            #[serde(default)]
            token: String,
            #[serde(rename = "bidFloor", default)]
            bid_floor: f64,
            #[serde(default)]
            instl: i32,
            #[serde(default)]
            exp: i64,
            #[serde(rename = "adUnitCode", default)]
            ad_unit_code: String,
        }

        let mut imp_ext: ExtImpAlkimi = match serde_json::from_value(bidder_val) {
            Ok(v) => v,
            Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
        };

        let mut new_imp = imp.clone();

        // Apply bid floor logic: use imp.bidfloor if set, otherwise use ext bid_floor
        if imp.bidfloor > 0.0 {
            new_imp.bidfloor = imp.bidfloor;
        } else {
            new_imp.bidfloor = imp_ext.bid_floor;
        }
        new_imp.instl = Some(imp_ext.instl);
        // exp field - set adUnitCode to imp.id
        imp_ext.ad_unit_code = imp.id.clone();

        let imp_ext_val = match serde_json::to_value(&imp_ext) {
            Ok(v) => v,
            Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
        };

        let mut new_bidder_ext = bidder_ext.clone();
        new_bidder_ext.insert("bidder".to_string(), imp_ext_val);

        new_imp.ext = match serde_json::to_value(&new_bidder_ext) {
            Ok(v) => Some(v),
            Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
        };

        updated.push(new_imp);
    }

    (updated, errs)
}

impl Bidder for AlkimiAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let (updated_imps, errs) = update_imps(&request.imp);

        // If any errors or counts don't match, return errors
        if !errs.is_empty() || updated_imps.len() != request.imp.len() {
            return (vec![], errs);
        }

        let mut req = request.clone();
        req.imp = updated_imps;

        let body = match serde_json::to_vec(&req) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

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
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        if bid_resp.seatbid.is_empty() {
            return Err(vec![BidderError::BadServerResponse("Empty SeatBid array".to_string())]);
        }

        let mut result = BidderResponse::with_capacity(internal.imp.len());
        let mut errs = Vec::new();

        for sb in bid_resp.seatbid {
            for mut bid in sb.bid {
                resolve_macros_bid(&mut bid);
                let imp_id = bid.impid.clone();
                match internal.imp.iter().find(|i| i.id == imp_id) {
                    Some(imp) => {
                        let bid_type = if imp.banner.is_some() {
                            BidType::Banner
                        } else if imp.video.is_some() {
                            BidType::Video
                        } else if imp.audio.is_some() {
                            BidType::Audio
                        } else {
                            errs.push(BidderError::BadInput(format!("Failed to find imp \"{}\"", imp_id)));
                            continue;
                        };
                        result.bids.push(TypedBid::new(bid, bid_type));
                    }
                    None => {
                        errs.push(BidderError::BadInput(format!("Failed to find imp \"{}\"", imp_id)));
                    }
                }
            }
        }

        Ok(result)
    }
}
