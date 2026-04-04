use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;

pub struct ThetradedeskAdapter {
    pub endpoint: String,
}

impl ThetradedeskAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

/// The Trade Desk bidder extension from imp.ext.bidder
#[derive(Debug, Default, Deserialize)]
struct ExtImpTheTradeDesk {
    #[serde(rename = "publisherId", default)]
    publisher_id: String,
    #[serde(rename = "supplySourceId", default)]
    supply_source_id: String,
}

fn get_bid_type_from_mtype(mtype: u32) -> Result<BidType, BidderError> {
    match mtype {
        1 => Ok(BidType::Banner),
        2 => Ok(BidType::Video),
        4 => Ok(BidType::Native),
        _ => Err(BidderError::BadServerResponse(format!("unsupported mtype: {}", mtype))),
    }
}

/// Replace ${AUCTION_PRICE} macro in bid fields
fn resolve_auction_price_macros(bid: &mut openrtb::Bid) {
    let price = format!("{}", bid.price);
    if let Some(nurl) = &bid.nurl {
        bid.nurl = Some(nurl.replace("${AUCTION_PRICE}", &price));
    }
    if let Some(adm) = &bid.adm {
        bid.adm = Some(adm.replace("${AUCTION_PRICE}", &price));
    }
    if let Some(burl) = &bid.burl {
        bid.burl = Some(burl.replace("${AUCTION_PRICE}", &price));
    }
}

fn get_extension_info(imps: &[openrtb::Imp]) -> Result<(String, String), BidderError> {
    let mut publisher_id = String::new();
    let mut supply_source_id = String::new();

    for imp in imps {
        let ext = imp.ext.as_ref()
            .and_then(|e| e.get("bidder"))
            .and_then(|b| serde_json::from_value::<ExtImpTheTradeDesk>(b.clone()).ok());

        if let Some(ttd_ext) = ext {
            if publisher_id.is_empty() && !ttd_ext.publisher_id.is_empty() {
                publisher_id = ttd_ext.publisher_id;
            }
            if supply_source_id.is_empty() && !ttd_ext.supply_source_id.is_empty() {
                supply_source_id = ttd_ext.supply_source_id;
            }
            if !publisher_id.is_empty() && !supply_source_id.is_empty() {
                break;
            }
        }
    }

    Ok((publisher_id, supply_source_id))
}

impl Bidder for ThetradedeskAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let (pub_id, supply_source_id) = match get_extension_info(&request.imp) {
            Ok(info) => info,
            Err(e) => return (vec![], vec![e]),
        };

        let mut req = request.clone();

        // For banner imps, use first format's size as w/h
        let mut modified_imps = Vec::new();
        for mut imp in req.imp {
            if let Some(banner) = &imp.banner {
                if let Some(formats) = &banner.format {
                    if let Some(first_format) = formats.first() {
                        let mut banner_copy = banner.clone();
                        banner_copy.w = first_format.w;
                        banner_copy.h = first_format.h;
                        imp.banner = Some(banner_copy);
                    }
                }
            }
            modified_imps.push(imp);
        }
        req.imp = modified_imps;

        // Set publisher ID on site or app
        if !pub_id.is_empty() {
            if let Some(site) = &mut req.site {
                let publisher = site.publisher.get_or_insert_with(Default::default);
                publisher.id = Some(pub_id.clone());
            } else if let Some(app) = &mut req.app {
                let publisher = app.publisher.get_or_insert_with(Default::default);
                publisher.id = Some(pub_id.clone());
            }
        }

        // Build endpoint URL: replace {{.SupplyId}} macro with supplySourceId if present
        let uri = if !supply_source_id.is_empty() {
            self.endpoint.replace("{{.SupplyId}}", &supply_source_id)
        } else {
            // Use endpoint as-is (with SupplyId macro replaced by empty string if needed)
            self.endpoint.replace("{{.SupplyId}}", "")
        };

        let body = match serde_json::to_vec(&req) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        (
            vec![RequestData {
                method: "POST".to_string(),
                uri,
                body,
                headers,
                imp_ids: get_imp_ids(&req.imp),
            }],
            vec![],
        )
    }

    fn make_bids(
        &self,
        _internal: &openrtb::BidRequest,
        _external: &RequestData,
        response: &ResponseData,
    ) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 {
            return Ok(BidderResponse::new());
        }
        if response.status_code == 400 {
            return Err(vec![BidderError::BadInput(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info",
                response.status_code
            ))]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info",
                response.status_code
            ))]);
        }

        let bid_response: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::new();
        if let Some(cur) = &bid_response.cur {
            if !cur.is_empty() {
                result.currency = cur.clone();
            }
        }

        for seat_bid in bid_response.seatbid {
            for mut bid in seat_bid.bid {
                resolve_auction_price_macros(&mut bid);

                let mtype = bid.mtype.unwrap_or(0) as u32;

                let bid_type = match get_bid_type_from_mtype(mtype) {
                    Ok(bt) => bt,
                    Err(e) => return Err(vec![e]),
                };

                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }

        Ok(result)
    }
}
