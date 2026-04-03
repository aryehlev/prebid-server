use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;

pub struct SeedingAllianceAdapter { pub endpoint: String }
impl SeedingAllianceAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ImpExtSeedingAlliance {
    #[serde(default)]
    ad_unit_id: String,
    #[serde(default)]
    seat_id: String,
    #[serde(default)]
    account_id: String,
}

fn get_bid_type_from_prebid_ext(ext: &Option<serde_json::Value>) -> Option<BidType> {
    let type_str = ext.as_ref()
        .and_then(|e| e.get("prebid"))
        .and_then(|p| p.get("type"))
        .and_then(|v| v.as_str())?;
    match type_str {
        "video" => Some(BidType::Video),
        "native" => Some(BidType::Native),
        "audio" => Some(BidType::Audio),
        _ => Some(BidType::Banner),
    }
}

fn resolve_price_macro(adm: &str, price: f64) -> String {
    adm.replace("${AUCTION_PRICE}", &format!("{}", price))
}

impl Bidder for SeedingAllianceAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut account_id = "pbs".to_string();
        let mut errs = Vec::new();
        let mut req_copy = request.clone();

        for imp in &mut req_copy.imp {
            let bidder_val = match imp.ext.as_ref().and_then(|e| e.get("bidder")) {
                Some(v) => v.clone(),
                None => {
                    errs.push(BidderError::BadInput("could not unmarshal adapters.ExtImpBidder: missing bidder".to_string()));
                    return (vec![], errs);
                }
            };
            let ext_sa: ImpExtSeedingAlliance = match serde_json::from_value(bidder_val) {
                Ok(e) => e,
                Err(e) => {
                    errs.push(BidderError::BadInput(format!("could not unmarshal openrtb_ext.ImpExtSeedingAlliance: {}", e)));
                    return (vec![], errs);
                }
            };
            imp.tagid = Some(ext_sa.ad_unit_id.clone());
            if !ext_sa.seat_id.is_empty() {
                account_id = ext_sa.seat_id.clone();
            }
            if !ext_sa.account_id.is_empty() {
                account_id = ext_sa.account_id.clone();
            }
        }

        // Ensure EUR is in the currency list
        let has_eur = req_copy.cur.as_ref().map_or(false, |c| c.iter().any(|s| s == "EUR"));
        if !has_eur {
            req_copy.cur.get_or_insert_with(Vec::new).push("EUR".to_string());
        }

        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        // Replace {{.AccountID}} in endpoint template
        let uri = self.endpoint.replace("{{.AccountID}}", &account_id);

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        (vec![RequestData {
            method: "POST".to_string(),
            uri,
            body,
            headers,
            imp_ids: get_imp_ids(&request.imp),
        }], errs)
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code == 400 {
            return Err(vec![BidderError::BadInput(
                "Unexpected status code: 400. Bad request from publisher. Run with request.debug = 1 for more info.".to_string()
            )]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info.", response.status_code
            ))]);
        }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(5);
        if let Some(cur) = &bid_resp.cur {
            if !cur.is_empty() { result.currency = cur.clone(); }
        }
        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for mut bid in sb.bid {
                if let Some(adm) = &bid.adm.clone() {
                    bid.adm = Some(resolve_price_macro(adm, bid.price));
                }
                let bid_type = match get_bid_type_from_prebid_ext(&bid.ext) {
                    Some(t) => t,
                    None => {
                        errs.push(BidderError::BadServerResponse("bid.Ext.Prebid is empty".to_string()));
                        continue;
                    }
                };
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        if !errs.is_empty() && result.bids.is_empty() { return Err(errs); }
        Ok(result)
    }
}
