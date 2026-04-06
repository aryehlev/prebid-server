use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;

pub struct ConsumableAdapter {
    pub endpoint: String,
}

impl ConsumableAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

/// Extension from imp.ext.bidder for Consumable
#[derive(Debug, Default, Deserialize)]
struct ExtImpConsumable {
    #[serde(rename = "siteId", default)]
    site_id: i64,
    #[serde(rename = "networkId", default)]
    network_id: i64,
    #[serde(rename = "unitId", default)]
    unit_id: i64,
    #[serde(rename = "placementId", default)]
    placement_id: String,
}

fn extract_consumable_ext(imp: &openrtb::Imp) -> Result<ExtImpConsumable, BidderError> {
    let bidder_val = imp.ext.as_ref()
        .and_then(|e| e.get("bidder"))
        .cloned()
        .ok_or_else(|| BidderError::BadInput("missing bidder ext".to_string()))?;

    serde_json::from_value(bidder_val)
        .map_err(|e| BidderError::BadInput(e.to_string()))
}

fn get_media_type_for_bid(bid: &openrtb::Bid) -> Result<BidType, BidderError> {
    // Use the top-level mtype field from the OpenRTB Bid struct.
    // OpenRTB mtype: 1=Banner, 2=Video, 3=Audio, 4=Native
    if let Some(mtype) = bid.mtype {
        match mtype {
            1 => return Ok(BidType::Banner),
            2 => return Ok(BidType::Video),
            3 => return Ok(BidType::Audio),
            _ => {}
        }
    }
    Err(BidderError::BadServerResponse(format!(
        "Failed to parse impression \"{}\" mediatype",
        bid.impid
    )))
}

impl Bidder for ConsumableAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::BadInput("no impressions".to_string())]);
        }

        let body = match serde_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        let imp_ids = get_imp_ids(&request.imp);

        let uri = if request.site.is_some() {
            // Site request: validate siteId/networkId/unitId
            match extract_consumable_ext(&request.imp[0]) {
                Ok(ext) => {
                    if ext.site_id == 0 && ext.network_id == 0 && ext.unit_id == 0 {
                        return (vec![], vec![BidderError::FailedToRequestBids(
                            "SiteId, NetworkId and UnitId are all required for site requests".to_string(),
                        )]);
                    }
                }
                Err(e) => return (vec![], vec![e]),
            }
            format!("{}/sb/rtb", self.endpoint)
        } else {
            // Non-site (app) request: requires placementId
            match extract_consumable_ext(&request.imp[0]) {
                Ok(ext) => {
                    if ext.placement_id.is_empty() {
                        return (vec![], vec![BidderError::FailedToRequestBids(
                            "PlacementId is required for non-site requests".to_string(),
                        )]);
                    }
                    format!("{}/rtb/bid?s={}", self.endpoint, ext.placement_id)
                }
                Err(e) => return (vec![], vec![e]),
            }
        };

        (
            vec![RequestData {
                method: "POST".to_string(),
                uri,
                body,
                headers,
                imp_ids,
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
        if let Err(e) = crate::check_response_status(response.status_code) {
            return Err(vec![e]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(bid_resp.seatbid.len());
        if let Some(cur) = &bid_resp.cur {
            if !cur.is_empty() {
                result.currency = cur.clone();
            }
        }

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = match get_media_type_for_bid(&bid) {
                    Ok(t) => t,
                    Err(_) => continue, // skip bids with unknown mtype
                };
                // Set mtype on bid based on bid_type (the Go code does this)
                let mut typed_bid = TypedBid::new(bid, bid_type.clone());
                if bid_type == BidType::Video {
                    // Extract duration from bid.dur (top-level OpenRTB field)
                    let dur = typed_bid.bid.dur.unwrap_or(0.0) as i32;
                    typed_bid.bid_video = Some(openrtb_ext::ExtBidPrebidVideo { duration: dur, primary_category: String::new() });
                }
                result.bids.push(typed_bid);
            }
        }
        Ok(result)
    }
}
