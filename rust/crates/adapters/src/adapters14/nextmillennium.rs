use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use openrtb_ext::BidType;
use serde::{Deserialize, Serialize};

pub struct NextmillenniumAdapter { pub endpoint: String }
impl NextmillenniumAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

#[derive(Debug, Deserialize, Default)]
struct ImpExtNextMillennium {
    #[serde(rename = "group_id", default)]
    group_id: String,
    #[serde(rename = "placement_id", default)]
    placement_id: String,
    #[serde(rename = "adSlots", default)]
    ad_slots: Vec<String>,
    #[serde(rename = "allowedAds", default)]
    allowed_ads: Vec<String>,
}

#[derive(Debug, Serialize, Default)]
struct NmExtPrebidStoredRequest {
    id: String,
}

#[derive(Debug, Serialize, Default)]
struct NmExtPrebid {
    storedrequest: NmExtPrebidStoredRequest,
}

#[derive(Debug, Serialize, Default)]
struct NmExtNmm {
    #[serde(rename = "adSlots", skip_serializing_if = "Vec::is_empty")]
    ad_slots: Vec<String>,
    #[serde(rename = "allowedAds", skip_serializing_if = "Vec::is_empty")]
    allowed_ads: Vec<String>,
}

#[derive(Debug, Serialize, Default)]
struct NextMillJsonExt {
    prebid: NmExtPrebid,
    #[serde(rename = "nextMillennium", skip_serializing_if = "is_nmm_empty")]
    next_millennium: NmExtNmm,
}

fn is_nmm_empty(nmm: &NmExtNmm) -> bool {
    nmm.ad_slots.is_empty() && nmm.allowed_ads.is_empty()
}

fn get_impression_ext(imp: &openrtb::Imp) -> Result<ImpExtNextMillennium, BidderError> {
    let ext = imp.ext.as_ref().ok_or_else(|| BidderError::BadInput("missing imp.ext".to_string()))?;
    let bidder_val = ext.get("bidder").ok_or_else(|| BidderError::BadInput("missing bidder ext".to_string()))?;
    serde_json::from_value(bidder_val.clone()).map_err(|e| BidderError::BadInput(e.to_string()))
}

impl Bidder for NextmillenniumAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();
        let mut errs = Vec::new();

        for (i, imp) in request.imp.iter().enumerate() {
            let nm_ext = match get_impression_ext(imp) {
                Ok(e) => e,
                Err(e) => { errs.push(e); continue; }
            };

            let placement_id = if !nm_ext.group_id.is_empty() {
                // Build placement ID from group_id + size + domain
                let domain = request.site.as_ref().map(|s| s.domain.as_deref().unwrap_or("")).unwrap_or("")
                    .to_string();
                let domain = if domain.is_empty() {
                    request.app.as_ref().map(|a| a.domain.as_deref().unwrap_or("")).unwrap_or("").to_string()
                } else {
                    domain
                };

                let size = if let Some(banner) = &request.imp[0].banner {
                    let formats = banner.format.as_deref().unwrap_or(&[]);
                    if !formats.is_empty() {
                        format!("{}x{}", formats[0].w.unwrap_or(0), formats[0].h.unwrap_or(0))
                    } else if banner.w.is_some() && banner.h.is_some() {
                        format!("{}x{}", banner.w.unwrap_or(0), banner.h.unwrap_or(0))
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                };

                format!("g{};{};{}", nm_ext.group_id, size, domain)
            } else {
                nm_ext.placement_id.clone()
            };

            let imp_ext = NextMillJsonExt {
                prebid: NmExtPrebid {
                    storedrequest: NmExtPrebidStoredRequest { id: placement_id },
                },
                next_millennium: NmExtNmm {
                    ad_slots: nm_ext.ad_slots,
                    allowed_ads: nm_ext.allowed_ads,
                },
            };

            let imp_ext_json = match serde_json::to_value(&imp_ext) {
                Ok(v) => v,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };

            let mut req_copy = request.clone();
            // Only use this impression (per-imp request)
            let mut imp_copy = request.imp[i].clone();
            imp_copy.ext = Some(imp_ext_json);
            req_copy.imp = vec![imp_copy];

            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };

            let mut headers = HashMap::new();
            headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
            headers.insert("Accept".to_string(), "application/json".to_string());
            headers.insert("x-openrtb-version".to_string(), "2.5".to_string());

            let imp_ids = req_copy.imp.iter().map(|i| i.id.clone()).collect();
            requests.push(RequestData {
                method: "POST".to_string(),
                uri: self.endpoint.clone(),
                body,
                headers,
                imp_ids,
            });
        }

        (requests, errs)
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected http status code: {}", response.status_code
            ))]);
        }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(format!("Bad server response: {}", e))])?;
        if bid_resp.seatbid.is_empty() { return Ok(BidderResponse::new()); }
        let mut result = BidderResponse::with_capacity(1);
        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = match bid.mtype.unwrap_or(0) {
                    1 => BidType::Banner,
                    2 => BidType::Video,
                    m => {
                        errs.push(BidderError::BadServerResponse(format!("Unsupported mType: {}", m)));
                        continue;
                    }
                };
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        if !errs.is_empty() && result.bids.is_empty() { return Err(errs); }
        Ok(result)
    }
}
