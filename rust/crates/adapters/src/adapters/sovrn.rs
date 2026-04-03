use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_bid_type_from_imp, get_imp_ids};
use openrtb_ext::BidType;
use serde::{Deserialize, Serialize};

pub struct SovrnAdapter {
    pub endpoint: String,
}

impl SovrnAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

#[derive(Serialize, Deserialize, Default)]
struct ExtImpSovrn {
    #[serde(rename = "tagid", default)]
    pub tagid: String,
    #[serde(rename = "TagId", default)]
    pub tag_id: String,
}

#[derive(Deserialize, Default)]
struct ImpExt {
    #[serde(default)]
    bidder: ExtImpSovrn,
}

impl Bidder for SovrnAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut valid_imps = Vec::new();

        for imp in &request.imp {
            // Parse bidder ext
            let imp_ext: ImpExt = match imp.ext.as_ref().and_then(|e| serde_json::from_value(e.clone()).ok()) {
                Some(e) => e,
                None => {
                    errs.push(BidderError::BadInput("Failed to parse sovrn imp ext".to_string()));
                    continue;
                }
            };

            let sovrn_ext = &imp_ext.bidder;

            // Get tagid — prefer lowercase tagid, fall back to TagId
            let tag_id = if !sovrn_ext.tagid.is_empty() {
                sovrn_ext.tagid.clone()
            } else {
                sovrn_ext.tag_id.clone()
            };

            if tag_id.is_empty() {
                errs.push(BidderError::BadInput("Missing required parameter 'tagid'".to_string()));
                continue;
            }

            let mut imp_copy = imp.clone();
            imp_copy.tagid = Some(tag_id);
            valid_imps.push(imp_copy);
        }

        if valid_imps.is_empty() {
            return (vec![], errs);
        }

        let mut req_copy = request.clone();
        req_copy.imp = valid_imps;

        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        // Add device headers if present
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
            if let Some(dnt) = &device.dnt {
                headers.insert("DNT".to_string(), dnt.to_string());
            }
        }

        // Add cookie header for buyeruid
        if let Some(user) = &request.user {
            if let Some(buyeruid) = &user.buyeruid {
                let uid = buyeruid.trim().to_string();
                if !uid.is_empty() {
                    headers.insert("Cookie".to_string(), format!("ljt_reader={}", uid));
                }
            }
        }

        let imp_ids = get_imp_ids(&req_copy.imp);

        (
            vec![RequestData {
                method: "POST".to_string(),
                uri: self.endpoint.clone(),
                body,
                headers,
                imp_ids,
            }],
            errs,
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
        if let Err(e) = crate::check_response_status(response.status_code) {
            return Err(vec![e]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(5);

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = internal
                    .imp
                    .iter()
                    .find(|i| i.id == bid.impid)
                    .map(get_bid_type_from_imp)
                    .unwrap_or(BidType::Banner);
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }

        Ok(result)
    }
}
