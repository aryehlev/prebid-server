use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct RevcontentAdapter {
    pub endpoint: String,
}

impl RevcontentAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

/// Determine bid type from the ad markup: HTML starts with '<' → Banner, otherwise Native.
fn get_bid_type_from_adm(adm: Option<&str>) -> BidType {
    match adm {
        Some(s) if s.starts_with('<') => BidType::Banner,
        _ => BidType::Native,
    }
}

impl Bidder for RevcontentAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        // Require app.name or site.domain
        let has_app_name = request
            .app
            .as_ref()
            .map(|a| !a.name.as_deref().unwrap_or("").is_empty())
            .unwrap_or(false);
        let has_site_domain = request
            .site
            .as_ref()
            .map(|s| !s.domain.as_deref().unwrap_or("").is_empty())
            .unwrap_or(false);

        if !has_app_name && !has_site_domain {
            return (
                vec![],
                vec![BidderError::BadInput(
                    "Impression is missing app name or site domain, and must contain one.".to_string(),
                )],
            );
        }

        let body = match serde_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());

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
        if let Err(e) = crate::check_response_status(response.status_code) {
            return Err(vec![e]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(5);

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = get_bid_type_from_adm(bid.adm.as_deref());
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }

        Ok(result)
    }
}
