use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct TeadsAdapter {
    pub endpoint: String,
}

impl TeadsAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

/// Modifies a clone of the request: sets banner H/W from first format entry,
/// validates placementId, and rewrites imp.ext to `{"kv":{"placementId":<id>}}`.
fn update_imp_objects(request: &mut openrtb::BidRequest) -> Result<(), BidderError> {
    for imp in request.imp.iter_mut() {
        // Set banner W/H from first format entry
        if let Some(banner) = imp.banner.as_mut() {
            if let Some(formats) = banner.format.as_deref() {
                if !formats.is_empty() {
                    let first = &formats[0];
                    banner.w = first.w;
                    banner.h = first.h;
                }
            }
        }

        // Parse placementId from imp.ext.bidder.placementId
        let placement_id: i64 = imp
            .ext
            .as_ref()
            .and_then(|e| e.get("bidder"))
            .and_then(|b| b.get("placementId"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        if placement_id == 0 {
            return Err(BidderError::BadInput(
                "placementId should not be 0.".to_string(),
            ));
        }

        // Set tagId to placement_id as string
        imp.tagid = Some(placement_id.to_string());

        // Rewrite ext to {"kv":{"placementId":<id>}}
        imp.ext = Some(serde_json::json!({
            "kv": {
                "placementId": placement_id
            }
        }));
    }
    Ok(())
}

fn get_media_type_for_imp(imp_id: &str, imps: &[openrtb::Imp]) -> Result<BidType, BidderError> {
    for imp in imps {
        if imp.id == imp_id {
            if imp.video.is_some() {
                return Ok(BidType::Video);
            }
            return Ok(BidType::Banner);
        }
    }
    Err(BidderError::BadInput("Imp ids were not equals".to_string()))
}

impl Bidder for TeadsAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (
                vec![],
                vec![BidderError::BadInput("No impression in the bid request".to_string())],
            );
        }

        let mut req = request.clone();
        if let Err(e) = update_imp_objects(&mut req) {
            return (vec![], vec![e]);
        }

        let body = match serde_json::to_vec(&req) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());

        (
            vec![RequestData {
                method: "POST".to_string(),
                uri: self.endpoint.clone(),
                body,
                headers,
                imp_ids: get_imp_ids(&request.imp),
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

        let mut result = BidderResponse::with_capacity(bid_resp.seatbid.len());

        if let Some(cur) = &bid_resp.cur {
            if !cur.is_empty() {
                result.currency = cur.clone();
            }
        }

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                // Extract renderer info from bid.ext.prebid.meta
                let renderer_name = bid.ext.as_ref()
                    .and_then(|e| e.get("prebid"))
                    .and_then(|p| p.get("meta"))
                    .and_then(|m| m.get("rendererName"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let renderer_version = bid.ext.as_ref()
                    .and_then(|e| e.get("prebid"))
                    .and_then(|p| p.get("meta"))
                    .and_then(|m| m.get("rendererVersion"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                if renderer_name.is_empty() {
                    return Err(vec![BidderError::BadInput(
                        "RendererName should not be empty if present".to_string(),
                    )]);
                }
                if renderer_version.is_empty() {
                    return Err(vec![BidderError::BadInput(
                        "RendererVersion should not be empty if present".to_string(),
                    )]);
                }

                let bid_type = get_media_type_for_imp(&bid.impid, &internal.imp)
                    .map_err(|e| vec![e])?;

                let mut typed_bid = TypedBid::new(bid, bid_type);
                typed_bid.bid_meta = Some(openrtb_ext::ExtBidPrebidMeta {
                    renderer_name: Some(renderer_name),
                    renderer_version: Some(renderer_version),
                    ..Default::default()
                });
                result.bids.push(typed_bid);
            }
        }

        Ok(result)
    }
}
