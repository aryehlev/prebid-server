use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;

pub struct GamoshiAdapter { pub endpoint: String }
impl GamoshiAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

#[derive(Debug, Default, Deserialize)]
struct ExtImpGamoshi {
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
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();

        if request.imp.is_empty() {
            return (vec![], vec![BidderError::BadInput("No impressions in the bid request".to_string())]);
        }

        let mut req_copy = request.clone();
        let mut valid_imp_exists = false;
        let mut i = 0usize;
        while i < req_copy.imp.len() {
            if req_copy.imp[i].banner.is_some() {
                let banner = req_copy.imp[i].banner.as_mut().unwrap();
                if banner.w.is_none() && banner.h.is_none() {
                    if let Some(fmt) = banner.format.as_deref().and_then(|f| f.first()).cloned() {
                        banner.w = fmt.w;
                        banner.h = fmt.h;
                    }
                }
                valid_imp_exists = true;
                i += 1;
            } else if req_copy.imp[i].video.is_some() {
                valid_imp_exists = true;
                i += 1;
            } else {
                errs.push(BidderError::BadInput(format!(
                    "Gamoshi only supports banner and video media types. Ignoring imp id={}",
                    req_copy.imp[i].id
                )));
                req_copy.imp.remove(i);
                // don't increment i
            }
        }

        if !valid_imp_exists {
            errs.push(BidderError::BadInput("No valid impression in the bid request".to_string()));
            return (vec![], errs);
        }

        let req_json = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => {
                errs.push(BidderError::BadInput(e.to_string()));
                return (vec![], errs);
            }
        };

        // Extract supplyPartnerId from first imp
        let bidder_val = match req_copy.imp[0].ext.as_ref().and_then(|e| e.get("bidder")).cloned() {
            Some(v) => v,
            None => return (vec![], vec![BidderError::BadInput("ext.bidder not provided".to_string())]),
        };
        let gamoshi_ext: ExtImpGamoshi = match serde_json::from_value(bidder_val) {
            Ok(e) => e,
            Err(_) => return (vec![], vec![BidderError::BadInput("ext.bidder.supplyPartnerId not provided".to_string())]),
        };
        if gamoshi_ext.supply_partner_id.is_empty() {
            return (vec![], vec![BidderError::BadInput("supplyPartnerId is empty".to_string())]);
        }

        let base_uri = if self.endpoint.is_empty() {
            "https://rtb.gamoshi.io".to_string()
        } else {
            self.endpoint.clone()
        };
        let uri = format!("{}/r/{}/bidr?bidder=prebid-server", base_uri, gamoshi_ext.supply_partner_id);

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
        headers.insert("x-openrtb-version".to_string(), "2.4".to_string());

        if let Some(device) = &request.device {
            if !device.ua.as_deref().unwrap_or("").is_empty() {
                headers.insert("User-Agent".to_string(), device.ua.as_deref().unwrap_or("").to_string());
            }
            if !device.ip.as_deref().unwrap_or("").is_empty() {
                headers.insert("X-Forwarded-For".to_string(), device.ip.as_deref().unwrap_or("").to_string());
            }
            if !device.language.as_deref().unwrap_or("").is_empty() {
                headers.insert("Accept-Language".to_string(), device.language.as_deref().unwrap_or("").to_string());
            }
            if let Some(dnt) = device.dnt {
                headers.insert("DNT".to_string(), dnt.to_string());
            }
        }

        let imp_ids = get_imp_ids(&req_copy.imp);
        (vec![RequestData {
            method: "POST".to_string(),
            uri,
            body: req_json,
            headers,
            imp_ids,
        }], errs)
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code == 400 {
            return Err(vec![BidderError::BadInput(format!(
                "Unexpected status code: {}. ", response.status_code
            ))]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "unexpected status code: {}. Run with request.debug = 1 for more info", response.status_code
            ))]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(format!("bad server response: {}. ", e))])?;

        if bid_resp.seatbid.is_empty() {
            return Ok(BidderResponse::new());
        }

        let cap = bid_resp.seatbid[0].bid.len();
        let mut result = BidderResponse::with_capacity(cap);
        let sb = &bid_resp.seatbid[0];
        for bid in &sb.bid {
            result.bids.push(TypedBid::new(bid.clone(), get_media_type(&bid.impid, &internal.imp)));
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_req() -> openrtb::BidRequest {
        openrtb::BidRequest {
            id: "r".to_string(),
            imp: vec![openrtb::Imp {
                id: "i1".to_string(),
                banner: Some(openrtb::Banner { w: Some(300), h: Some(250), ..Default::default() }),
                ext: Some(serde_json::json!({"bidder": {"supplyPartnerId": "1707"}})),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn test_make_requests_basic() {
        let a = GamoshiAdapter::new("https://rtb.gamoshi.io".to_string());
        let (reqs, errs) = a.make_requests(&make_req(), &ExtraRequestInfo::default());
        assert!(errs.is_empty());
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].uri, "https://rtb.gamoshi.io/r/1707/bidr?bidder=prebid-server");
        assert_eq!(reqs[0].headers.get("x-openrtb-version").unwrap(), "2.4");
    }

    #[test]
    fn test_make_bids_basic() {
        let a = GamoshiAdapter::new("https://rtb.gamoshi.io".to_string());
        let body = br#"{"id":"r","seatbid":[{"bid":[{"id":"b1","impid":"i1","price":1.0}]}]}"#;
        let resp = ResponseData::new(200, body.to_vec());
        let result = a.make_bids(&make_req(), &RequestData::default(), &resp).unwrap();
        assert_eq!(result.bids.len(), 1);
        assert_eq!(result.bids[0].bid_type, BidType::Banner);
    }
}
