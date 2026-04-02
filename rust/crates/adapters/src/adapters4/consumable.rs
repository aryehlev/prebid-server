use std::collections::HashMap;
use pbs_adapters::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct ConsumableAdapter {
    pub endpoint: String,
}

impl ConsumableAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

/// Minimal ext structures for Consumable imp extension parsing.
#[derive(serde::Deserialize, Default)]
struct ExtImpBidder {
    #[serde(default)]
    bidder: ConsumableExt,
}

#[derive(serde::Deserialize, Default)]
struct ConsumableExt {
    #[serde(rename = "siteId", default)]
    site_id: i64,
    #[serde(rename = "networkId", default)]
    network_id: i64,
    #[serde(rename = "unitId", default)]
    unit_id: i64,
    #[serde(rename = "placementId", default)]
    placement_id: String,
}

impl Bidder for ConsumableAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let body = match serde_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        // Parse ext from first impression.
        let consumable_ext: ConsumableExt = request.imp.first()
            .and_then(|imp| imp.ext.as_ref())
            .and_then(|ext| serde_json::from_str::<ExtImpBidder>(ext.get()).ok())
            .map(|e| e.bidder)
            .unwrap_or_default();

        let uri = if request.site.is_some() {
            // Site request: validate required fields.
            if consumable_ext.site_id == 0 && consumable_ext.network_id == 0 && consumable_ext.unit_id == 0 {
                return (vec![], vec![BidderError::BadInput(
                    "SiteId, NetworkId and UnitId are all required for site requests".to_string(),
                )]);
            }
            format!("{}/sb/rtb", self.endpoint)
        } else {
            // Non-site (app) request: needs placementId.
            if consumable_ext.placement_id.is_empty() {
                return (vec![], vec![BidderError::BadInput(
                    "PlacementId is required for non-site requests".to_string(),
                )]);
            }
            format!("{}/rtb/bid?s={}", self.endpoint, consumable_ext.placement_id)
        };

        (
            vec![RequestData {
                method: "POST".to_string(),
                uri,
                body,
                headers,
                imp_ids: get_imp_ids(&request.imp),
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
        if let Err(e) = pbs_adapters::check_response_status(response.status_code) {
            return Err(vec![e]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(5);
        if let Some(cur) = &bid_resp.cur {
            result.currency = cur.clone();
        }

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                // Determine bid type from mtype field.
                let bid_type = match bid.mtype {
                    Some(1) => BidType::Banner,
                    Some(2) => BidType::Video,
                    Some(3) => BidType::Audio,
                    _ => continue, // skip unknown media types matching Go behaviour
                };
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        Ok(result)
    }
}
