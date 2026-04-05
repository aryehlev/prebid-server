use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use openrtb_ext::BidType;
use serde::{Deserialize, Serialize};

pub struct ConnatixAdapter { pub endpoint: String }
impl ConnatixAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

#[derive(Debug, Deserialize, Clone)]
struct ImpExtBidder {
    #[serde(rename = "placementId", default)]
    placement_id: String,
    #[serde(rename = "viewabilityPercentage", default)]
    viewability_percentage: Option<f64>,
}

#[derive(Debug, Deserialize, Clone)]
struct ImpExtIncoming {
    bidder: Option<ImpExtBidder>,
}

#[derive(Debug, Serialize, Clone)]
struct ImpExtConnatix {
    #[serde(rename = "placementId")]
    placement_id: String,
    #[serde(rename = "viewabilityPercentage", skip_serializing_if = "Option::is_none")]
    viewability_percentage: Option<f64>,
}

#[derive(Debug, Deserialize, Clone, Default)]
struct BidExtCnx {
    #[serde(rename = "mediaType", default)]
    media_type: String,
}

#[derive(Debug, Deserialize, Clone)]
struct BidExtConnatix {
    cnx: Option<BidExtCnx>,
}

fn get_bid_type_from_ext(ext: &Option<serde_json::Value>) -> BidType {
    if let Some(e) = ext {
        if let Ok(bid_ext) = serde_json::from_value::<BidExtConnatix>(e.clone()) {
            if let Some(cnx) = bid_ext.cnx {
                if cnx.media_type == "video" {
                    return BidType::Video;
                }
            }
        }
    }
    BidType::Banner
}

impl Bidder for ConnatixAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let has_ip = request.device.as_ref().map(|d| d.ip.as_deref().map_or(false, |s| !s.is_empty()) || d.ipv6.as_deref().map_or(false, |s| !s.is_empty())).unwrap_or(false);
        if !has_ip {
            return (vec![], vec![BidderError::BadInput("Device IP is required".to_string())]);
        }

        let mut errs = Vec::new();
        let mut valid_imps: Vec<openrtb::Imp> = Vec::new();

        for imp in &request.imp {
            let ext_val = match imp.ext.as_ref() {
                Some(e) => e.clone(),
                None => {
                    errs.push(BidderError::BadInput("missing imp ext".to_string()));
                    continue;
                }
            };

            let imp_ext: ImpExtIncoming = match serde_json::from_value(ext_val.clone()) {
                Ok(e) => e,
                Err(e) => {
                    errs.push(BidderError::BadInput(e.to_string()));
                    continue;
                }
            };

            let bidder = match imp_ext.bidder {
                Some(b) => b,
                None => {
                    errs.push(BidderError::BadInput("missing bidder in imp ext".to_string()));
                    continue;
                }
            };

            let mut imp_copy = imp.clone();

            // Set banner w/h from first format if not set
            if let Some(ref mut banner) = imp_copy.banner {
                if banner.w.is_none() && banner.h.is_none() {
                    if let Some(formats) = &banner.format {
                        if let Some(first) = formats.first() {
                            banner.w = Some(first.w.unwrap_or(0));
                            banner.h = Some(first.h.unwrap_or(0));
                        }
                    }
                }
            }

            // Build new ext: keep non-bidder fields, add connatix key
            let mut new_ext: HashMap<String, serde_json::Value> = if let Some(existing) = &imp.ext {
                serde_json::from_value(existing.clone()).unwrap_or_default()
            } else {
                HashMap::new()
            };
            new_ext.remove("bidder");
            new_ext.insert("connatix".to_string(), serde_json::to_value(ImpExtConnatix {
                placement_id: bidder.placement_id,
                viewability_percentage: bidder.viewability_percentage,
            }).unwrap_or(serde_json::Value::Null));

            imp_copy.ext = Some(serde_json::to_value(&new_ext).unwrap_or(serde_json::Value::Null));
            valid_imps.push(imp_copy);
        }

        if valid_imps.is_empty() {
            return (vec![], errs);
        }

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        if let Some(device) = &request.device {
            if let Some(ua) = &device.ua { if !ua.is_empty() {
                headers.insert("User-Agent".to_string(), ua.clone());
            }}
            if let Some(ipv6) = &device.ipv6 { if !ipv6.is_empty() {
                headers.insert("X-Forwarded-For".to_string(), ipv6.clone());
            }}
            if let Some(ip) = &device.ip { if !ip.is_empty() {
                headers.insert("X-Forwarded-For".to_string(), ip.clone());
            }}
        }

        // Split into one-imp-per-request
        let mut requests = Vec::new();
        for imp in valid_imps {
            let imp_id = imp.id.clone();
            let mut req_copy = request.clone();
            req_copy.imp = vec![imp];

            // Determine dc query param from user buyeruid
            let mut uri = self.endpoint.clone();
            if let Some(user) = &request.user {
                let buyer_uid_opt = user.buyeruid.as_deref().unwrap_or("").trim().to_string();
                let buyer_uid = buyer_uid_opt.as_str();
                if !buyer_uid.is_empty() {
                    let dc = if buyer_uid.starts_with("1-") {
                        Some("us-east-2")
                    } else if buyer_uid.starts_with("2-") {
                        Some("us-west-2")
                    } else if buyer_uid.starts_with("3-") {
                        Some("eu-west-1")
                    } else {
                        None
                    };
                    if let Some(dc_val) = dc {
                        if uri.contains('?') {
                            uri.push_str(&format!("&dc={}", dc_val));
                        } else {
                            uri.push_str(&format!("?dc={}", dc_val));
                        }
                    }
                }
            }

            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };

            requests.push(RequestData {
                method: "POST".to_string(),
                uri,
                body,
                headers: headers.clone(),
                imp_ids: vec![imp_id],
            });
        }

        (requests, errs)
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(1);
        result.currency = "USD".to_string();

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = get_bid_type_from_ext(&bid.ext);
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        Ok(result)
    }
}
