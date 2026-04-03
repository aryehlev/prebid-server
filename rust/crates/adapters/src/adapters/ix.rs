use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_bid_type_from_imp, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;

pub struct IxAdapter {
    pub endpoint: String,
}

impl IxAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

#[derive(Deserialize, Default)]
struct ExtImpIx {
    #[serde(rename = "siteId", default)]
    pub site_id: String,
}

#[derive(Deserialize, Default)]
struct ImpExt {
    #[serde(default)]
    bidder: ExtImpIx,
}

impl Bidder for IxAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut unique_site_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut valid_imps = Vec::new();

        for imp in &request.imp {
            let imp_ext: ImpExt = match imp.ext.as_ref().and_then(|e| serde_json::from_value(e.clone()).ok()) {
                Some(e) => e,
                None => {
                    errs.push(BidderError::BadInput("Failed to parse ix imp ext".to_string()));
                    continue;
                }
            };

            if !imp_ext.bidder.site_id.is_empty() {
                unique_site_ids.insert(imp_ext.bidder.site_id.clone());
            }

            valid_imps.push(imp.clone());
        }

        if valid_imps.is_empty() {
            return (vec![], errs);
        }

        let mut req_copy = request.clone();
        req_copy.imp = valid_imps;

        // Set publisher ID from site_id if only one unique site ID
        if unique_site_ids.len() == 1 {
            let site_id = unique_site_ids.iter().next().unwrap().clone();

            if let Some(site) = &req_copy.site {
                let mut site_copy = site.clone();
                let mut pub_copy = site_copy.publisher.clone().unwrap_or_default();
                pub_copy.id = Some(site_id.clone());
                site_copy.publisher = Some(pub_copy);
                req_copy.site = Some(site_copy);
            } else if let Some(app) = &req_copy.app {
                let mut app_copy = app.clone();
                let mut pub_copy = app_copy.publisher.clone().unwrap_or_default();
                pub_copy.id = Some(site_id);
                app_copy.publisher = Some(pub_copy);
                req_copy.app = Some(app_copy);
            }
        }

        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => {
                errs.push(BidderError::BadInput(e.to_string()));
                return (vec![], errs);
            }
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
        headers.insert("X-OpenRTB-Version".to_string(), "2.5".to_string());

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
