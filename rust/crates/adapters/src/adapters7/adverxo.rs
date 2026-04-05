use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use openrtb_ext::BidType;
use serde::Deserialize;
use serde_json::Value;

pub struct AdverxoAdapter { pub endpoint: String }
impl AdverxoAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

#[derive(Deserialize)]
struct ExtImpBidder { bidder: Value }

#[derive(Deserialize)]
struct ImpExtAdverxo {
    #[serde(rename = "adUnitId")]
    ad_unit_id: i32,
    #[serde(default)]
    auth: String,
}

const PRICE_MACRO: &str = "${AUCTION_PRICE}";

fn mtype_to_bid_type(mtype: Option<i32>) -> Result<BidType, BidderError> {
    match mtype {
        Some(1) => Ok(BidType::Banner),
        Some(4) => Ok(BidType::Native),
        Some(2) => Ok(BidType::Video),
        _ => Err(BidderError::BadServerResponse(format!("unsupported MType {:?}", mtype))),
    }
}

/// For native bids, if adm wraps the native object in a "native" key, unwrap it.
/// Mirrors Go's getNativeAdm function.
fn get_native_adm(adm: &str) -> Result<String, BidderError> {
    let parsed: serde_json::Value = serde_json::from_str(adm)
        .map_err(|_| BidderError::BadServerResponse("unable to unmarshal native adm".to_string()))?;
    if let Some(native_val) = parsed.get("native") {
        if native_val.is_object() {
            serde_json::to_string(native_val)
                .map_err(|_| BidderError::BadServerResponse("unable to get native adm".to_string()))
        } else {
            Err(BidderError::BadServerResponse("unable to get native adm".to_string()))
        }
    } else {
        Ok(adm.to_string())
    }
}

/// Replace ${AUCTION_PRICE} macro in bid adm with the actual price.
/// Mirrors Go's resolveMacros function.
fn resolve_macros(bid: &mut openrtb::Bid) {
    let price = format!("{}", bid.price);
    if let Some(adm) = &bid.adm {
        bid.adm = Some(adm.replace(PRICE_MACRO, &price));
    }
}

impl Bidder for AdverxoAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut requests = Vec::new();
        for imp in &request.imp {
            let bidder_ext: ExtImpBidder = match imp.ext.as_ref()
                .and_then(|e| serde_json::from_value(e.clone()).ok()) {
                Some(v) => v,
                None => { errs.push(BidderError::BadInput(format!("imp {}: unable to unmarshal ext", imp.id))); continue; }
            };
            let ext: ImpExtAdverxo = match serde_json::from_value(bidder_ext.bidder) {
                Ok(v) => v,
                Err(e) => { errs.push(BidderError::BadInput(format!("imp {}: unable to unmarshal ext.bidder: {}", imp.id, e))); continue; }
            };
            let uri = self.endpoint.replace("{{.AdUnit}}", &ext.ad_unit_id.to_string())
                .replace("{{.TokenID}}", &ext.auth);
            let mut req = request.clone();
            req.imp = vec![imp.clone()];
            // Note: bid floor currency conversion (requestInfo.ConvertCurrency) is not
            // available in the Rust infrastructure; floor values are passed as-is.
            let body = match serde_json::to_vec(&req) {
                Ok(b) => b,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };
            requests.push(RequestData { method: "POST".to_string(), uri, body, headers: HashMap::new(), imp_ids: vec![imp.id.clone()] });
        }
        (requests, errs)
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!("Unexpected status code: {}. Run with request.debug = 1 for more info.", response.status_code))]);
        }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(5);
        if let Some(cur) = &bid_resp.cur { result.currency = cur.clone(); }
        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for mut bid in sb.bid {
                match mtype_to_bid_type(bid.mtype) {
                    Ok(bt) => {
                        // For native bids, fix the adm field by unwrapping "native" wrapper
                        if bt == BidType::Native {
                            if let Some(adm) = &bid.adm.clone() {
                                match get_native_adm(adm) {
                                    Ok(fixed_adm) => bid.adm = Some(fixed_adm),
                                    Err(e) => { errs.push(e); continue; }
                                }
                            }
                        }
                        // Replace ${AUCTION_PRICE} macro in adm
                        resolve_macros(&mut bid);
                        result.bids.push(TypedBid::new(bid, bt));
                    }
                    Err(e) => errs.push(e),
                }
            }
        }
        if errs.is_empty() {
            Ok(result)
        } else {
            // Return error on first bid type error (mirrors Go returning early on error)
            Err(errs)
        }
    }
}
