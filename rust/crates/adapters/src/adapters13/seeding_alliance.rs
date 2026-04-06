use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids, check_response_status};
use openrtb::BidResponse;
use openrtb_ext::BidType;

pub struct SeedingAllianceAdapter { pub endpoint: String }
impl SeedingAllianceAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

fn parse_bid_type(type_str: &str) -> Result<BidType, BidderError> {
    match type_str {
        "banner" => Ok(BidType::Banner),
        "video" => Ok(BidType::Video),
        "native" => Ok(BidType::Native),
        _ => Err(BidderError::BadServerResponse(format!("invalid bid type: {}", type_str))),
    }
}

fn get_media_type_for_bid(ext: Option<&serde_json::Value>) -> Result<BidType, BidderError> {
    let type_str = ext
        .and_then(|e| e.get("prebid"))
        .and_then(|p| p.get("type"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if type_str.is_empty() {
        return Err(BidderError::BadServerResponse("bid.Ext.Prebid is empty".to_string()));
    }
    parse_bid_type(type_str)
}

/// Get accountId from imp.ext.bidder: seatId or accountId, default "pbs"
fn get_ext_info(imp: &mut openrtb::Imp) -> Result<String, BidderError> {
    let bidder = imp.ext.as_ref()
        .and_then(|e| e.get("bidder"))
        .cloned()
        .ok_or_else(|| BidderError::BadInput("could not unmarshal adapters.ExtImpBidder".to_string()))?;

    let ad_unit_id = bidder.get("adUnitId")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if !ad_unit_id.is_empty() {
        imp.tagid = Some(ad_unit_id);
    }

    let seat_id = bidder.get("seatId")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if !seat_id.is_empty() {
        return Ok(seat_id);
    }

    let account_id = bidder.get("accountId")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if !account_id.is_empty() {
        return Ok(account_id);
    }

    Ok("pbs".to_string())
}

fn cur_exists(currencies: &Option<Vec<String>>, target: &str) -> bool {
    currencies.as_ref()
        .map(|c| c.iter().any(|x| x == target))
        .unwrap_or(false)
}

fn build_url(template: &str, account_id: &str) -> String {
    template.replace("{{.AccountID}}", account_id)
}

fn resolve_price_macro(adm: &str, price: f64) -> String {
    let price_str = format!("{}", price);
    adm.replace("${AUCTION_PRICE}", &price_str)
}

impl Bidder for SeedingAllianceAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut req_copy = request.clone();
        let mut account_id = "pbs".to_string();

        for imp in &mut req_copy.imp {
            match get_ext_info(imp) {
                Ok(id) => account_id = id,
                Err(e) => return (vec![], vec![e]),
            }
        }

        // Ensure EUR is in the currencies list
        if !cur_exists(&req_copy.cur, "EUR") {
            let mut cur = req_copy.cur.unwrap_or_default();
            cur.push("EUR".to_string());
            req_copy.cur = Some(cur);
        }

        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let url = build_url(&self.endpoint, &account_id);
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());

        let imp_ids = get_imp_ids(&req_copy.imp);
        (vec![RequestData {
            method: "POST".to_string(),
            uri: url,
            body,
            headers,
            imp_ids,
        }], vec![])
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(5);
        if let Some(cur) = &bid_resp.cur {
            if !cur.is_empty() { result.currency = cur.clone(); }
        }
        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for mut bid in sb.bid {
                // Resolve price macro in adm
                if let Some(adm) = &bid.adm {
                    bid.adm = Some(resolve_price_macro(adm, bid.price));
                }
                match get_media_type_for_bid(bid.ext.as_ref()) {
                    Ok(t) => result.bids.push(TypedBid::new(bid, t)),
                    Err(e) => errs.push(e),
                }
            }
        }
        if !errs.is_empty() { return Err(errs); }
        Ok(result)
    }
}
