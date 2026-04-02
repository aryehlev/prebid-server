use std::collections::HashMap;
use pbs_adapters::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

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
            let zone_id = imp.ext.as_ref()
                .and_then(|e| e.get("bidder"))
                .and_then(|b| b.get("zoneId").or_else(|| b.get("zone_id")))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            match zone_id {
                Some(ref z) if z.len() >= 32 && z.len() <= 36 => {
                    let mut imp_copy = imp.clone();
                    imp_copy.tagid = Some(z.clone());

                    // Apply custom bid floor from ext if imp has no floor
                    let custom_floor = imp.ext.as_ref()
                        .and_then(|e| e.get("bidder"))
                        .and_then(|b| b.get("customBidFloor"))
                        .and_then(|v| v.as_f64());
                    if imp_copy.bidfloor.unwrap_or(0.0) == 0.0 {
                        if let Some(floor) = custom_floor {
                            if floor > 0.0 {
                                imp_copy.bidfloor = Some(floor);
                            }
                        }
                    }
                    imp_copy.ext = None;
                    valid_imps.push(imp_copy);
                }
                Some(ref z) if z.len() < 32 || z.len() > 36 => {
                    errs.push(BidderError::BadInput("ZoneId must be UUID/GUID".to_string()));
                }
                _ => {
                    errs.push(BidderError::BadInput("Wrong Admixer bidder ext".to_string()));
                }
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
