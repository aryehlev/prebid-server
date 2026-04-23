use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids, check_response_status};
use openrtb_ext::BidType;

pub struct PangleAdapter {
    pub endpoint: String,
}

impl PangleAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

fn get_ad_type(imp: &openrtb::Imp, is_rewarded: bool) -> i32 {
    if imp.video.is_some() {
        if is_rewarded {
            return 7;
        }
        if imp.instl == Some(1) {
            return 8;
        }
    }
    if imp.banner.is_some() {
        if imp.instl == Some(1) {
            return 2;
        }
        return 1;
    }
    if let Some(native) = &imp.native {
        if native.request.as_deref().map(|r| !r.is_empty()).unwrap_or(false) {
            return 5;
        }
    }
    -1
}

fn get_media_type_for_ad_type(ad_type: i32) -> Result<BidType, BidderError> {
    match ad_type {
        1 | 2 => Ok(BidType::Banner),
        5 => Ok(BidType::Native),
        7 | 8 => Ok(BidType::Video),
        _ => Err(BidderError::BadServerResponse(
            "unrecognized adtype in response".to_string(),
        )),
    }
}

impl Bidder for PangleAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut requests = Vec::new();

        for imp in &request.imp {
            // Check if rewarded from imp.ext.prebid.is_rewarded_inventory
            let is_rewarded = imp
                .ext
                .as_ref()
                .and_then(|e| e.get("prebid"))
                .and_then(|p| p.get("is_rewarded_inventory"))
                .and_then(|v| v.as_i64())
                .unwrap_or(0)
                == 1;

            let ad_type = get_ad_type(imp, is_rewarded);
            if ad_type == -1 {
                errs.push(BidderError::BadInput("not a supported adtype".to_string()));
                continue;
            }

            // Get bidder ext fields
            let bidder = imp
                .ext
                .as_ref()
                .and_then(|e| e.get("bidder"))
                .cloned()
                .unwrap_or(serde_json::Value::Null);

            let token = bidder
                .get("token")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let app_id = bidder
                .get("appid")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let placement_id = bidder
                .get("placementid")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            // Validate: must have both appid and placementid or neither
            let has_app = !app_id.is_empty();
            let has_placement = !placement_id.is_empty();
            if has_app != has_placement {
                errs.push(BidderError::BadInput(
                    "only one of appid or placementid is provided".to_string(),
                ));
                continue;
            }

            // Build new imp ext with adtype, is_prebid, networkids
            let mut new_ext = serde_json::json!({
                "adtype": ad_type,
                "is_prebid": true,
            });
            if has_app && has_placement {
                new_ext["networkids"] = serde_json::json!({
                    "appid": app_id,
                    "placementid": placement_id,
                });
            }
            // Carry through prebid ext if present
            if let Some(prebid) = imp.ext.as_ref().and_then(|e| e.get("prebid")) {
                new_ext["prebid"] = prebid.clone();
            }

            let mut imp_copy = imp.clone();
            imp_copy.ext = Some(new_ext);

            let mut req_copy = request.clone();
            req_copy.imp = vec![imp_copy];

            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(BidderError::BadInput(e.to_string()));
                    continue;
                }
            };

            let mut headers = HashMap::new();
            headers.insert("TOKEN".to_string(), token);
            headers.insert("Content-Type".to_string(), "application/json".to_string());

            let imp_ids = get_imp_ids(&req_copy.imp);
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

    fn make_bids(
        &self,
        _internal: &openrtb::BidRequest,
        _external: &RequestData,
        response: &ResponseData,
    ) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 {
            return Ok(BidderResponse::new());
        }
        if let Err(e) = check_response_status(response.status_code) {
            return Err(vec![e]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(1);
        if let Some(cur) = &bid_resp.cur {
            if !cur.is_empty() {
                result.currency = cur.clone();
            }
        }

        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                // Get adtype from bid.ext.pangle.adtype
                let ad_type = bid
                    .ext
                    .as_ref()
                    .and_then(|e| e.get("pangle"))
                    .and_then(|p| p.get("adtype"))
                    .and_then(|v| v.as_i64())
                    .map(|v| v as i32)
                    .unwrap_or(-1);

                match get_media_type_for_ad_type(ad_type) {
                    Ok(t) => result.bids.push(TypedBid::new(bid, t)),
                    Err(e) => errs.push(e),
                }
            }
        }

        if !errs.is_empty() {
            return Err(errs);
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_make_requests_sets_token_header_and_adtype() {
        let adapter = PangleAdapter::new("https://pangle.example/rtb".to_string());
        let mut req = openrtb::BidRequest::default();
        req.id = "r".to_string();
        req.imp = vec![openrtb::Imp {
            id: "imp1".to_string(),
            banner: Some(Default::default()),
            ext: Some(serde_json::json!({"bidder":{"token":"TK","appid":"A","placementid":"P"}})),
            ..Default::default()
        }];
        let info = ExtraRequestInfo::default();
        let (requests, errs) = adapter.make_requests(&req, &info);
        assert!(errs.is_empty());
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].headers.get("TOKEN").map(String::as_str), Some("TK"));
        let parsed: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
        assert_eq!(parsed["imp"][0]["ext"]["adtype"], 1);
        assert_eq!(parsed["imp"][0]["ext"]["is_prebid"], true);
        assert_eq!(parsed["imp"][0]["ext"]["networkids"]["appid"], "A");
    }
}
