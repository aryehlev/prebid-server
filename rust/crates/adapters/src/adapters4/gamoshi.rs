use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct GamoshiAdapter {
    pub endpoint: String,
}

impl GamoshiAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

#[derive(serde::Deserialize)]
struct ExtImpBidder {
    bidder: GamoshiExt,
}

#[derive(serde::Deserialize)]
struct GamoshiExt {
    #[serde(rename = "supplyPartnerId", default)]
    supply_partner_id: String,
}

fn get_media_type(imp_id: &str, imps: &[openrtb::Imp]) -> BidType {
    for imp in imps {
        if imp.id == imp_id {
            if imp.video.is_some() {
                return BidType::Video;
            }
            return BidType::Banner;
        }
    }
    BidType::Banner
}

impl Bidder for GamoshiAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (
                vec![],
                vec![BidderError::BadInput("No impressions in the bid request".to_string())],
            );
        }

        let mut errors = Vec::new();
        let mut req_copy = request.clone();
        let mut valid_imp_exists = false;

        // Filter/process imps — only banner and video supported.
        let mut valid_imps = Vec::new();
        for imp in &req_copy.imp {
            if let Some(banner) = &imp.banner {
                let mut imp_copy = imp.clone();
                let mut banner_copy = banner.clone();
                if banner_copy.w.is_none() && banner_copy.h.is_none() && !banner_copy.format.is_empty() {
                    let first = banner_copy.format[0].clone();
                    banner_copy.w = Some(first.w);
                    banner_copy.h = Some(first.h);
                }
                imp_copy.banner = Some(banner_copy);
                valid_imps.push(imp_copy);
                valid_imp_exists = true;
            } else if imp.video.is_some() {
                valid_imps.push(imp.clone());
                valid_imp_exists = true;
            } else {
                errors.push(BidderError::BadInput(format!(
                    "Gamoshi only supports banner and video media types. Ignoring imp id={}",
                    imp.id
                )));
            }
        }

        if !valid_imp_exists {
            errors.push(BidderError::BadInput(
                "No valid impression in the bid request".to_string(),
            ));
            return (vec![], errors);
        }

        req_copy.imp = valid_imps;

        // Parse supplyPartnerId from first imp ext.
        let supply_partner_id = match req_copy.imp.first()
            .and_then(|imp| imp.ext.as_ref())
            .and_then(|ext| serde_json::from_str::<ExtImpBidder>(ext.get()).ok())
        {
            Some(e) => {
                if e.bidder.supply_partner_id.is_empty() {
                    return (
                        vec![],
                        vec![BidderError::BadInput("supplyPartnerId is empty".to_string())],
                    );
                }
                e.bidder.supply_partner_id
            }
            None => {
                return (
                    vec![],
                    vec![BidderError::BadInput("ext.bidder not provided".to_string())],
                );
            }
        };

        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => {
                return (vec![], vec![BidderError::BadInput(e.to_string())]);
            }
        };

        let base = if self.endpoint.is_empty() {
            "https://rtb.gamoshi.io".to_string()
        } else {
            self.endpoint.clone()
        };
        let uri = format!("{}/r/{}/bidr?bidder=prebid-server", base, supply_partner_id);

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
        headers.insert("x-openrtb-version".to_string(), "2.4".to_string());

        if let Some(device) = &request.device {
            if let Some(ua) = &device.ua {
                if !ua.is_empty() {
                    headers.insert("User-Agent".to_string(), ua.clone());
                }
            }
            if let Some(ip) = &device.ip {
                if !ip.is_empty() {
                    headers.insert("X-Forwarded-For".to_string(), ip.clone());
                }
            }
            if let Some(lang) = &device.language {
                if !lang.is_empty() {
                    headers.insert("Accept-Language".to_string(), lang.clone());
                }
            }
            if let Some(dnt) = device.dnt {
                headers.insert("DNT".to_string(), dnt.to_string());
            }
        }

        (
            vec![RequestData {
                method: "POST".to_string(),
                uri,
                body,
                headers,
                imp_ids: get_imp_ids(&req_copy.imp),
            }],
            errors,
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
        if response.status_code == 400 {
            return Err(vec![BidderError::BadInput(format!(
                "Unexpected status code: {}. ",
                response.status_code
            ))]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "unexpected status code: {}. Run with request.debug = 1 for more info",
                response.status_code
            ))]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        if bid_resp.seatbid.is_empty() {
            return Ok(BidderResponse::new());
        }

        let mut result = BidderResponse::with_capacity(5);
        let sb = &bid_resp.seatbid[0];
        for bid in &sb.bid {
            result.bids.push(TypedBid::new(
                bid.clone(),
                get_media_type(&bid.impid, &internal.imp),
            ));
        }
        Ok(result)
    }
}
