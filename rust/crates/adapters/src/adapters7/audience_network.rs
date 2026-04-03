use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;
use serde_json::Value;

pub struct AudienceNetworkAdapter {
    pub endpoint: String,
}

impl AudienceNetworkAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

#[derive(Deserialize)]
struct ExtImpBidder {
    bidder: Value,
}

#[derive(Deserialize)]
struct ExtImpFacebook {
    #[serde(rename = "placementId", default)]
    placement_id: String,
    #[serde(rename = "publisherId", default)]
    publisher_id: String,
}

#[derive(Deserialize)]
struct FacebookAdMarkup {
    #[serde(rename = "bid_id", default)]
    bid_id: String,
}

fn get_bid_type_for_imp(imp: &openrtb::Imp) -> BidType {
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

        let buyer_uid = request.user.as_ref()
            .and_then(|u| u.buyeruid.as_deref())
            .unwrap_or("");
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

        let mut results = Vec::new();
        let mut errs = Vec::new();

        for imp in &request.imp {
            // Extract placement and publisher from imp.ext
            let ext_val = match &imp.ext {
                Some(v) => v.clone(),
                None => { errs.push(BidderError::BadInput("missing imp ext".to_string())); continue; }
            };

            let bidder_ext: ExtImpBidder = match serde_json::from_value(ext_val) {
                Ok(v) => v,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };

            let fb_ext: ExtImpFacebook = match serde_json::from_value(bidder_ext.bidder) {
                Ok(v) => v,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };

            if fb_ext.placement_id.is_empty() {
                errs.push(BidderError::BadInput("Missing placementId param".to_string()));
                continue;
            }

            // Parse placement_id: can be "pubId_placementId" or just placementId (require publisherId separately)
            let (placement_id, publisher_id) = {
                let toks: Vec<&str> = fb_ext.placement_id.split('_').collect();
                match toks.len() {
                    1 => {
                        if fb_ext.publisher_id.is_empty() {
                            errs.push(BidderError::BadInput("Missing publisherId param".to_string()));
                            continue;
                        }
                        (fb_ext.placement_id.clone(), fb_ext.publisher_id.clone())
                    }
                    2 => (toks[1].to_string(), toks[0].to_string()),
                    _ => {
                        errs.push(BidderError::BadInput(format!(
                            "Invalid placementId param '{}' and publisherId param '{}'",
                            fb_ext.placement_id, fb_ext.publisher_id
                        )));
                        continue;
                    }
                }
            };

            let tag_id = format!("{}_{}", publisher_id, placement_id);

            // Build single-imp request
            let mut req_copy = request.clone();
            req_copy.id = imp.id.clone();
            req_copy.imp = vec![{
                let mut imp_copy = imp.clone();
                imp_copy.tagid = Some(tag_id);
                imp_copy.ext = None;
                imp_copy
            }];

            // Set app.publisher.id
            if let Some(app) = &mut req_copy.app {
                let mut app_copy = app.clone();
                if app_copy.publisher.is_none() {
                    app_copy.publisher = Some(openrtb::Publisher::default());
                }
                if let Some(pub_) = &mut app_copy.publisher {
                    pub_.id = Some(publisher_id.clone());
                }
                req_copy.app = Some(app_copy);
            }

            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };

            results.push(RequestData {
                method: "POST".to_string(),
                uri: self.endpoint.clone(),
                body,
                headers: headers.clone(),
                imp_ids: get_imp_ids(&req_copy.imp),
            });
        }

        (results, errs)
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadInput(format!(
                "Unexpected status code {} with error message ''", response.status_code
            ))]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(4);
        let mut errs = Vec::new();

        for sb in bid_resp.seatbid {
            for mut bid in sb.bid {
                let adm = match &bid.adm {
                    Some(adm) if !adm.is_empty() => adm.clone(),
                    _ => {
                        errs.push(BidderError::BadServerResponse(format!("Bid {} missing 'adm'", bid.id)));
                        continue;
                    }
                };

                let ad_markup: FacebookAdMarkup = match serde_json::from_str(&adm) {
                    Ok(m) => m,
                    Err(e) => {
                        errs.push(BidderError::BadServerResponse(e.to_string()));
                        continue;
                    }
                };

                if ad_markup.bid_id.is_empty() {
                    errs.push(BidderError::BadServerResponse(format!("bid {} missing 'bid_id' in 'adm'", bid.id)));
                    continue;
                }

                bid.adid = Some(ad_markup.bid_id.clone());
                bid.crid = Some(ad_markup.bid_id.clone());

                let bid_type = internal.imp.iter()
                    .find(|i| i.id == bid.impid)
                    .map(get_bid_type_for_imp)
                    .unwrap_or(BidType::Banner);

                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }

        Ok(result)
    }
}
