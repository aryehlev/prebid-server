use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct JixieAdapter {
    pub endpoint: String,
}

impl JixieAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

/// Determine bid type from the ad markup (adm). If it contains XML/VAST markers → Video.
fn get_bid_type(adm: Option<&str>) -> BidType {
    if let Some(adm_str) = adm {
        let lower = adm_str.to_lowercase();
        if lower.contains("<?xml") || lower.contains("<vast") {
            return BidType::Video;
        }
    }
    BidType::Banner
}

impl Bidder for JixieAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
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

        if let Some(site) = &request.site {
            if let Some(page) = &site.page {
                if !page.is_empty() {
                    headers.insert("Referer".to_string(), page.clone());
                }
            }
        }

        (
            vec![RequestData {
                method: "POST".to_string(),
                uri: self.endpoint.clone(),
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
        if response.status_code == 400 {
            return Err(vec![BidderError::BadInput(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info",
                response.status_code
            ))]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Invalid Status Returned: {}. Run with request.debug = 1 for more info",
                response.status_code
            ))]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body).map_err(|e| {
            vec![BidderError::BadServerResponse(format!(
                "Unable to unpackage bid response. Error: {}",
                e
            ))]
        })?;

        let mut bids: Vec<TypedBid> = Vec::new();

        for sb in bid_resp.seatbid {
            for mut bid in sb.bid {
                // Jixie sets imp_id = bid.id
                bid.impid = bid.id.clone();
                let bid_type = get_bid_type(bid.adm.as_deref());
                bids.push(TypedBid::new(bid, bid_type));
            }
        }

        let mut result = BidderResponse::with_capacity(bids.len());
        // Currency: use response cur if set, else default "USD"
        result.currency = bid_resp.cur.unwrap_or_else(|| "USD".to_string());
        result.bids = bids;

        Ok(result)
    }
}
