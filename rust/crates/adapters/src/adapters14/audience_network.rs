use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct AudienceNetworkAdapter { pub endpoint: String }
impl AudienceNetworkAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

fn resolve_imp_type(imp: &openrtb::Imp) -> BidType {
    if imp.banner.is_some() { return BidType::Banner; }
    if imp.video.is_some() { return BidType::Video; }
    if imp.audio.is_some() { return BidType::Audio; }
    if imp.native.is_some() { return BidType::Native; }
    BidType::Banner
}

impl Bidder for AudienceNetworkAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::BadInput("No impressions provided".to_string())]);
        }

        let buyer_uid = request.user.as_ref().and_then(|u| u.buyeruid.as_deref()).unwrap_or("");
        if buyer_uid.is_empty() {
            return (vec![], vec![BidderError::BadInput("Missing bidder token in 'user.buyeruid'".to_string())]);
        }

        if request.site.is_some() {
            return (vec![], vec![BidderError::BadInput("Site impressions are not supported.".to_string())]);
        }

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
        headers.insert("X-Fb-Pool-Routing-Token".to_string(), buyer_uid.to_string());

        let mut requests = Vec::new();
        let mut errs = Vec::new();

        for imp in &request.imp {
            // Extract placementId and publisherId from imp.ext.bidder
            let bidder_val = match imp.ext.as_ref().and_then(|e| e.get("bidder")) {
                Some(v) => v.clone(),
                None => {
                    errs.push(BidderError::BadInput("Missing placementId param".to_string()));
                    continue;
                }
            };

            let placement_id = bidder_val.get("placementId").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if placement_id.is_empty() {
                errs.push(BidderError::BadInput("Missing placementId param".to_string()));
                continue;
            }

            let (final_placement_id, publisher_id) = {
                let toks: Vec<&str> = placement_id.splitn(3, '_').collect();
                if toks.len() == 1 {
                    let pub_id = bidder_val.get("publisherId").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    if pub_id.is_empty() {
                        errs.push(BidderError::BadInput("Missing publisherId param".to_string()));
                        continue;
                    }
                    (placement_id.clone(), pub_id)
                } else if toks.len() == 2 {
                    (toks[1].to_string(), toks[0].to_string())
                } else {
                    errs.push(BidderError::BadInput(format!("Invalid placementId param '{}'", placement_id)));
                    continue;
                }
            };

            // Validate banner height if banner impression
            if let Some(banner) = &imp.banner {
                if banner.h.is_none() {
                    let supported = [50i32, 250i32];
                    let found = banner.format.as_deref().unwrap_or(&[]).iter()
                        .any(|f| f.h.map_or(false, |h| supported.contains(&h)));
                    if !found {
                        errs.push(BidderError::BadInput(format!("imp #{}: banner height required", imp.id)));
                        continue;
                    }
                } else if let Some(h) = banner.h {
                    if h != 50 && h != 250 && imp.instl != Some(1) {
                        errs.push(BidderError::BadInput(format!("imp #{}: only banner heights 50 and 250 are supported", imp.id)));
                        continue;
                    }
                }
            }

            // Build per-imp request
            let mut req_copy = request.clone();
            req_copy.id = imp.id.clone();
            let mut imp_copy = imp.clone();
            imp_copy.tagid = Some(format!("{}_{}", publisher_id, final_placement_id));
            imp_copy.ext = None;

            if let Some(app) = &req_copy.app {
                let mut app_copy = app.clone();
                let mut publisher = app_copy.publisher.clone().unwrap_or_default();
                publisher.id = Some(publisher_id.clone());
                app_copy.publisher = Some(publisher);
                req_copy.app = Some(app_copy);
            }

            req_copy.imp = vec![imp_copy];

            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };

            requests.push(RequestData {
                method: "POST".to_string(),
                uri: self.endpoint.clone(),
                body,
                headers: headers.clone(),
                imp_ids: get_imp_ids(&req_copy.imp),
            });
        }

        (requests, errs)
    }

    fn make_bids(&self, request: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code != 200 {
            let msg = format!("Unexpected status code {} with error message ''", response.status_code);
            return Err(vec![BidderError::BadInput(msg)]);
        }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(4);
        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let adm = bid.adm.as_deref().unwrap_or("");
                if adm.is_empty() {
                    errs.push(BidderError::BadServerResponse(format!("Bid {} missing 'adm'", bid.id)));
                    continue;
                }
                let bid_id_val: serde_json::Value = match serde_json::from_str(adm) {
                    Ok(v) => v,
                    Err(e) => { errs.push(BidderError::BadServerResponse(e.to_string())); continue; }
                };
                let obj_bid_id = bid_id_val.get("bid_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                if obj_bid_id.is_empty() {
                    errs.push(BidderError::BadServerResponse(format!("bid {} missing 'bid_id' in 'adm'", bid.id)));
                    continue;
                }
                let mut bid_copy = bid.clone();
                bid_copy.adid = Some(obj_bid_id.clone());
                bid_copy.crid = Some(obj_bid_id);
                let bid_type = request.imp.iter().find(|i| i.id == bid_copy.impid)
                    .map(|i| resolve_imp_type(i))
                    .unwrap_or(BidType::Banner);
                result.bids.push(TypedBid::new(bid_copy, bid_type));
            }
        }
        if result.bids.is_empty() && !errs.is_empty() {
            return Err(errs);
        }
        Ok(result)
    }
}
