use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;

pub struct MadvertiseAdapter { pub endpoint: String }
impl MadvertiseAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

#[derive(Deserialize, Default)]
struct MadvertiseImpExt {
    #[serde(rename = "zone_id", alias = "zoneId", default)]
    zone_id: String,
}

/// adcom1 creative attributes that indicate video: 6=VideoAuto, 7=VideoUser, 16=HasSkipButton
fn get_media_type_for_bid(attr: Option<&Vec<i32>>) -> BidType {
    if let Some(attrs) = attr {
        for &a in attrs {
            if a == 16 || a == 6 || a == 7 {
                return BidType::Video;
            }
        }
    }
    BidType::Banner
}

fn get_impression_ext(imp: &openrtb::Imp) -> Result<MadvertiseImpExt, BidderError> {
    let ext = imp.ext.as_ref().ok_or_else(|| BidderError::BadInput(format!("missing ext; ImpID={}", imp.id)))?;
    let bidder_val = ext.get("bidder").ok_or_else(|| BidderError::BadInput(format!("missing bidder ext; ImpID={}", imp.id)))?;
    let ext_parsed: MadvertiseImpExt = serde_json::from_value(bidder_val.clone())
        .map_err(|e| BidderError::BadInput(format!("{}; ImpID={}", e, imp.id)))?;
    if ext_parsed.zone_id.is_empty() {
        return Err(BidderError::BadInput(format!("ext.bidder.zoneId not provided; ImpID={}", imp.id)));
    }
    Ok(ext_parsed)
}

impl Bidder for MadvertiseAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut zone_id = String::new();

        for imp in &request.imp {
            let imp_ext = match get_impression_ext(imp) {
                Ok(e) => e,
                Err(e) => return (vec![], vec![e]),
            };
            if imp_ext.zone_id.len() < 7 {
                return (vec![], vec![BidderError::BadInput(format!(
                    "The minLength of zone ID is 7; ImpID={}", imp.id
                ))]);
            }
            if zone_id.is_empty() {
                zone_id = imp_ext.zone_id;
            } else if zone_id != imp_ext.zone_id {
                return (vec![], vec![BidderError::BadInput("There must be only one zone ID".to_string())]);
            }
        }

        // Build URL: replace {{.ZoneID}} macro in endpoint template
        let uri = self.endpoint.replace("{{.ZoneID}}", &zone_id);

        let body = match serde_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
        headers.insert("X-Openrtb-Version".to_string(), "2.5".to_string());

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
        }

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

    fn make_bids(&self, _internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 {
            return Ok(BidderResponse::new());
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info",
                response.status_code
            ))]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(5);
        if let Some(cur) = bid_resp.cur {
            result.currency = cur;
        }

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = get_media_type_for_bid(bid.attr.as_ref());
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }

        Ok(result)
    }
}
