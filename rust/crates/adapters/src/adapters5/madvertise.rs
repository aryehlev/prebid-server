use std::collections::HashMap;
use pbs_adapters::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct MadvertiseAdapter {
    pub endpoint: String,
}

impl MadvertiseAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

/// Extract zone_id from imp.ext.bidder.zoneId.
fn get_zone_id(imp: &openrtb::Imp) -> Result<String, BidderError> {
    let ext = imp
        .ext
        .as_ref()
        .ok_or_else(|| BidderError::BadInput(format!("imp.ext missing; ImpID={}", imp.id)))?;
    let bidder = ext
        .get("bidder")
        .ok_or_else(|| {
            BidderError::BadInput(format!("imp.ext.bidder missing; ImpID={}", imp.id))
        })?;
    let zone_id = bidder
        .get("zoneId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            BidderError::BadInput(format!(
                "ext.bidder.zoneId not provided; ImpID={}",
                imp.id
            ))
        })?
        .to_string();

    if zone_id.is_empty() {
        return Err(BidderError::BadInput(format!(
            "ext.bidder.zoneId not provided; ImpID={}",
            imp.id
        )));
    }

    if zone_id.len() < 7 {
        return Err(BidderError::BadInput(format!(
            "The minLength of zone ID is 7; ImpID={}",
            imp.id
        )));
    }

    Ok(zone_id)
}

/// Determine bid type from creative attributes (adcom1 attribute values).
/// AttrHasSkipButton=16, AttrVideoAuto=6, AttrVideoUser=7.
fn get_media_type_for_bid(attr: Option<&Vec<serde_json::Value>>) -> BidType {
    // Video-indicating creative attribute values from adcom1
    const ATTR_VIDEO_AUTO: i64 = 6;
    const ATTR_VIDEO_USER: i64 = 7;
    const ATTR_HAS_SKIP_BUTTON: i64 = 16;

    if let Some(attrs) = attr {
        for a in attrs {
            if let Some(n) = a.as_i64() {
                if n == ATTR_VIDEO_AUTO || n == ATTR_VIDEO_USER || n == ATTR_HAS_SKIP_BUTTON {
                    return BidType::Video;
                }
            }
        }
    }
    BidType::Banner
}

impl Bidder for MadvertiseAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut zone_id = String::new();

        for imp in &request.imp {
            let zid = match get_zone_id(imp) {
                Ok(z) => z,
                Err(e) => return (vec![], vec![e]),
            };

            if zone_id.is_empty() {
                zone_id = zid;
            } else if zone_id != zid {
                return (
                    vec![],
                    vec![BidderError::BadInput(
                        "There must be only one zone ID".to_string(),
                    )],
                );
            }
        }

        // Build endpoint URL by substituting {{.ZoneID}} macro
        let uri = self.endpoint.replace("{{.ZoneID}}", &zone_id);

        let body = match serde_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let mut headers = HashMap::new();
        headers.insert(
            "Content-Type".to_string(),
            "application/json;charset=utf-8".to_string(),
        );
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

    fn make_bids(
        &self,
        _internal: &openrtb::BidRequest,
        _external: &RequestData,
        response: &ResponseData,
    ) -> Result<BidderResponse, Vec<BidderError>> {
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
        if let Some(cur) = &bid_resp.cur {
            result.currency = cur.clone();
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
