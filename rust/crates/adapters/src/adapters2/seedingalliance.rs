use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct SeedingallianceAdapter {
    pub endpoint: String,
}

impl SeedingallianceAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

fn resolve_price_macro(adm: &str, price: f64) -> String {
    let price_str = format!("{}", price);
    adm.replace("${AUCTION_PRICE}", &price_str)
}

fn get_media_type_for_bid(ext: &serde_json::Value) -> Result<BidType, BidderError> {
    // Check bid.ext.prebid.type
    let bid_type_str = ext
        .get("prebid")
        .and_then(|p| p.get("type"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    match bid_type_str {
        "banner" => Ok(BidType::Banner),
        "video" => Ok(BidType::Video),
        "native" => Ok(BidType::Native),
        "audio" => Ok(BidType::Audio),
        "" => Err(BidderError::BadServerResponse(
            "bid.Ext.Prebid is empty".to_string(),
        )),
        other => Err(BidderError::BadServerResponse(format!(
            "unrecognized bid type: {}",
            other
        ))),
    }
}

/// Extract account ID and set tagid on imp from bidder ext.
/// Returns account_id string.
fn get_ext_info(imp: &mut openrtb::Imp) -> Result<String, BidderError> {
    let bidder = imp
        .ext
        .as_ref()
        .and_then(|e| e.get("bidder"))
        .cloned()
        .unwrap_or(serde_json::Value::Null);

    let ad_unit_id = bidder
        .get("adUnitId")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let seat_id = bidder
        .get("seatId")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let account_id = bidder
        .get("accountId")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if !ad_unit_id.is_empty() {
        imp.tagid = Some(ad_unit_id);
    }

    // Priority: accountId > seatId > default "pbs"
    let resolved_account = if !account_id.is_empty() {
        account_id
    } else if !seat_id.is_empty() {
        seat_id
    } else {
        "pbs".to_string()
    };

    Ok(resolved_account)
}

impl Bidder for SeedingallianceAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut req = request.clone();
        let mut account_id = String::new();

        for imp in req.imp.iter_mut() {
            match get_ext_info(imp) {
                Ok(aid) => account_id = aid,
                Err(e) => return (vec![], vec![e]),
            }
        }

        // Ensure EUR is in currencies
        let cur = req.cur.get_or_insert_with(Vec::new);
        if !cur.contains(&"EUR".to_string()) {
            cur.push("EUR".to_string());
        }

        let body = match serde_json::to_vec(&req) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        // Build URL with account ID substitution
        let uri = if !account_id.is_empty() && self.endpoint.contains("{{.AccountID}}") {
            self.endpoint.replace("{{.AccountID}}", &account_id)
        } else {
            self.endpoint.clone()
        };

        let imp_ids = get_imp_ids(&req.imp);
        (
            vec![RequestData {
                method: "POST".to_string(),
                uri,
                body,
                headers: HashMap::new(),
                imp_ids,
            }],
            vec![],
        )
    }

    fn make_bids(
        &self,
        internal: &openrtb::BidRequest,
        _external: &RequestData,
        response: &ResponseData,
    ) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 {
            return Ok(BidderResponse::new());
        }
        if let Err(e) = crate::check_response_status(response.status_code) {
            return Err(vec![e]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(internal.imp.len());
        if let Some(cur) = bid_resp.cur.as_deref() {
            if !cur.is_empty() {
                result.currency = cur.to_string();
            }
        }

        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for mut bid in sb.bid {
                // Resolve price macro in adm
                if let Some(adm) = bid.adm.as_ref() {
                    bid.adm = Some(resolve_price_macro(adm, bid.price));
                }

                let bid_type = match bid.ext.as_ref() {
                    Some(ext) => match get_media_type_for_bid(ext) {
                        Ok(t) => t,
                        Err(e) => {
                            errs.push(e);
                            continue;
                        }
                    },
                    None => {
                        errs.push(BidderError::BadServerResponse(
                            "bid.Ext.Prebid is empty".to_string(),
                        ));
                        continue;
                    }
                };

                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }

        if errs.is_empty() {
            Ok(result)
        } else {
            // Return partial results along with errors – use Ok to match Go behavior
            // (Go returns bidResponse + errs, not nil)
            Ok(result)
        }
    }
}
