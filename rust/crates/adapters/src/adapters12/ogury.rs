use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids, check_response_status};
use openrtb::BidResponse;
use openrtb_ext::BidType;

pub struct OguryAdapter { pub endpoint: String }
impl OguryAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

fn get_bid_type_from_mtype(mtype: u64, imp_id: &str) -> Result<BidType, BidderError> {
    match mtype {
        1 => Ok(BidType::Banner),
        2 => Ok(BidType::Video),
        3 => Ok(BidType::Audio),
        4 => Ok(BidType::Native),
        _ => Err(BidderError::BadServerResponse(
            format!("Unsupported MType \"{}\", for impression \"{}\"", mtype, imp_id)
        )),
    }
}

fn has_publisher_id(request: &openrtb::BidRequest) -> bool {
    let site_ok = request.site.as_ref()
        .and_then(|s| s.publisher.as_ref())
        .and_then(|p| p.id.as_ref())
        .map(|id| !id.is_empty())
        .unwrap_or(false);
    let app_ok = request.app.as_ref()
        .and_then(|a| a.publisher.as_ref())
        .and_then(|p| p.id.as_ref())
        .map(|id| !id.is_empty())
        .unwrap_or(false);
    site_ok || app_ok
}

impl Bidder for OguryAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _extra: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut req_copy = request.clone();
        let mut errs = Vec::new();
        let mut imps_with_ogury_params: Vec<openrtb::Imp> = Vec::new();

        for (i, imp) in request.imp.iter().enumerate() {
            // Parse imp.ext as a map so we can hoist bidder params
            let mut imp_ext: HashMap<String, serde_json::Value> = match imp.ext.as_ref() {
                Some(e) => match serde_json::from_value(e.clone()) {
                    Ok(m) => m,
                    Err(_) => {
                        errs.push(BidderError::BadInput("Bidder extension not provided or can't be unmarshalled".to_string()));
                        return (vec![], errs);
                    }
                },
                None => {
                    errs.push(BidderError::BadInput("Bidder extension not provided or can't be unmarshalled".to_string()));
                    return (vec![], errs);
                }
            };

            // Extract bidder extension params and hoist them into imp.ext directly
            let bidder_params: HashMap<String, serde_json::Value> = imp_ext
                .get("bidder")
                .and_then(|b| serde_json::from_value(b.clone()).ok())
                .unwrap_or_default();

            let has_asset_key = bidder_params.contains_key("assetKey");
            let has_ad_unit_id = bidder_params.contains_key("adUnitId");

            for (k, v) in bidder_params {
                imp_ext.insert(k, v);
            }
            imp_ext.remove("bidder");

            let new_ext = serde_json::to_value(&imp_ext)
                .unwrap_or(serde_json::Value::Null);

            let mut imp_copy = imp.clone();
            imp_copy.ext = Some(new_ext);
            imp_copy.tagid = Some(imp.id.clone());

            req_copy.imp[i] = imp_copy.clone();

            if has_asset_key && has_ad_unit_id {
                imps_with_ogury_params.push(imp_copy);
            }
        }

        if imps_with_ogury_params.is_empty() {
            if !has_publisher_id(request) {
                return (vec![], vec![BidderError::BadInput(
                    "Invalid request. assetKey/adUnitId or request.site/app.publisher.id required".to_string()
                )]);
            }
        } else {
            req_copy.imp = imps_with_ogury_params;
        }

        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        if let Some(device) = &request.device {
            if let Some(ip) = &device.ip {
                if !ip.is_empty() {
                    headers.insert("X-Forwarded-For".to_string(), ip.clone());
                }
            }
            if let Some(ipv6) = &device.ipv6 {
                if !ipv6.is_empty() {
                    // Go adds both, we only insert once unless different header
                    headers.insert("X-Forwarded-For-IPv6".to_string(), ipv6.clone());
                }
            }
            if let Some(ua) = &device.ua {
                if !ua.is_empty() {
                    headers.insert("User-Agent".to_string(), ua.clone());
                }
            }
            if let Some(lang) = &device.language {
                if !lang.is_empty() {
                    headers.insert("Accept-Language".to_string(), lang.clone());
                }
            }
        }

        let imp_ids = get_imp_ids(&req_copy.imp);
        (vec![RequestData {
            method: "POST".to_string(),
            uri: self.endpoint.clone(),
            body,
            headers,
            imp_ids,
        }], errs)
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(5);
        if let Some(cur) = bid_resp.cur {
            result.currency = cur;
        }
        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let mtype = bid.mtype.unwrap_or(0) as u64;
                match get_bid_type_from_mtype(mtype, &bid.impid) {
                    Ok(t) => result.bids.push(TypedBid::new(bid, t)),
                    Err(e) => errs.push(e),
                }
            }
        }
        if !errs.is_empty() { return Err(errs); }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_make_requests_hoists_bidder_params() {
        let adapter = OguryAdapter::new("https://ogury.example/rtb".to_string());
        let mut req = openrtb::BidRequest::default();
        req.id = "r".to_string();
        req.imp = vec![openrtb::Imp {
            id: "imp1".to_string(),
            banner: Some(Default::default()),
            ext: Some(serde_json::json!({"bidder":{"assetKey":"AK","adUnitId":"AU"}})),
            ..Default::default()
        }];
        let info = ExtraRequestInfo::default();
        let (requests, errs) = adapter.make_requests(&req, &info);
        assert!(errs.is_empty());
        assert_eq!(requests.len(), 1);
        let parsed: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
        assert_eq!(parsed["imp"][0]["ext"]["assetKey"], "AK");
        assert_eq!(parsed["imp"][0]["ext"]["adUnitId"], "AU");
        assert_eq!(parsed["imp"][0]["tagid"], "imp1");
    }

    #[test]
    fn test_make_requests_requires_params_or_publisher_id() {
        let adapter = OguryAdapter::new("https://ogury.example/rtb".to_string());
        let mut req = openrtb::BidRequest::default();
        req.imp = vec![openrtb::Imp {
            id: "imp1".to_string(),
            banner: Some(Default::default()),
            ext: Some(serde_json::json!({"bidder":{}})),
            ..Default::default()
        }];
        let info = ExtraRequestInfo::default();
        let (requests, errs) = adapter.make_requests(&req, &info);
        assert!(requests.is_empty());
        assert!(!errs.is_empty());
    }
}
