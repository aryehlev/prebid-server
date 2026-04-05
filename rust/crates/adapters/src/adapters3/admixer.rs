use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde_json::Value;

pub struct AdmixerAdapter {
    pub endpoint: String,
}

impl AdmixerAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

fn get_media_type_for_imp(imp_id: &str, imps: &[openrtb::Imp]) -> BidType {
    for imp in imps {
        if imp.id == imp_id {
            if imp.banner.is_some() {
                return BidType::Banner;
            } else if imp.video.is_some() {
                return BidType::Video;
            } else if imp.native.is_some() {
                return BidType::Native;
            } else if imp.audio.is_some() {
                return BidType::Audio;
            }
        }
    }
    BidType::Banner
}

/// Preprocesses an imp: validates zoneId, sets tagid, applies custom bid floor,
/// and rewrites ext to only contain customParams (if present).
/// Go ext fields: "zone" for ZoneId, "customFloor" for CustomBidFloor, "customParams" for CustomParams.
fn preprocess_imp(imp: &openrtb::Imp) -> Result<openrtb::Imp, BidderError> {
    let bidder_ext = imp.ext.as_ref()
        .and_then(|e| e.get("bidder"))
        .ok_or_else(|| BidderError::BadInput("Wrong Admixer bidder ext".to_string()))?;

    let zone_id = bidder_ext
        .get("zone")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if zone_id.is_empty() {
        return Err(BidderError::BadInput("Wrong Admixer bidder ext".to_string()));
    }

    // ZoneId must be UUID/GUID: 32-36 characters
    if zone_id.len() < 32 || zone_id.len() > 36 {
        return Err(BidderError::BadInput("ZoneId must be UUID/GUID".to_string()));
    }

    let custom_bid_floor = bidder_ext
        .get("customFloor")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    let custom_params = bidder_ext
        .get("customParams")
        .cloned();

    let mut imp_copy = imp.clone();
    imp_copy.tagid = Some(zone_id);

    // Apply custom bid floor only if imp has no floor set
    if imp_copy.bidfloor.unwrap_or(0.0) == 0.0 && custom_bid_floor > 0.0 {
        imp_copy.bidfloor = Some(custom_bid_floor);
    }

    // Rewrite ext: null if no customParams, otherwise {"customParams": ...}
    imp_copy.ext = match custom_params {
        Some(params) if !params.is_null() => {
            let mut obj = serde_json::Map::new();
            obj.insert("customParams".to_string(), params);
            Some(Value::Object(obj))
        }
        _ => None,
    };

    Ok(imp_copy)
}

impl Bidder for AdmixerAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::BadInput("No impressions in request".to_string())]);
        }

        let mut errs = Vec::new();
        let mut valid_imps: Vec<openrtb::Imp> = Vec::new();

        for imp in &request.imp {
            match preprocess_imp(imp) {
                Ok(processed) => valid_imps.push(processed),
                Err(e) => errs.push(e),
            }
        }

        if valid_imps.is_empty() {
            return (vec![], errs);
        }

        let mut req = request.clone();
        req.imp = valid_imps;

        let body = match serde_json::to_vec(&req) {
            Ok(b) => b,
            Err(e) => {
                errs.push(BidderError::BadInput(e.to_string()));
                return (vec![], errs);
            }
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        (
            vec![RequestData {
                method: "POST".to_string(),
                uri: self.endpoint.clone(),
                body,
                headers,
                imp_ids: get_imp_ids(&req.imp),
            }],
            errs,
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
        if response.status_code >= 500 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected status code: {}. Dsp server internal error", response.status_code
            ))]);
        }
        if response.status_code >= 400 {
            return Err(vec![BidderError::BadInput(format!(
                "Unexpected status code: {}. Bad request to dsp", response.status_code
            ))]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected status code: {}", response.status_code
            ))]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        // Additional no-content check: no seatbids or no bids in first seatbid
        if bid_resp.seatbid.is_empty() || bid_resp.seatbid[0].bid.is_empty() {
            return Ok(BidderResponse::new());
        }

        let mut result = BidderResponse::with_capacity(bid_resp.seatbid[0].bid.len());

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = get_media_type_for_imp(&bid.impid, &internal.imp);
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }

        Ok(result)
    }
}
