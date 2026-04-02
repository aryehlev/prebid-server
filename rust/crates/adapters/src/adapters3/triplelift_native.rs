use std::collections::HashMap;
use pbs_adapters::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct TripleliftNativeAdapter {
    pub endpoint: String,
    /// Publisher IDs allowed to use this adapter. Empty means all publishers allowed.
    pub publisher_whitelist: Vec<String>,
}

impl TripleliftNativeAdapter {
    pub fn new(endpoint: String) -> Self {
        Self {
            endpoint,
            publisher_whitelist: Vec::new(),
        }
    }

    pub fn with_whitelist(endpoint: String, publisher_whitelist: Vec<String>) -> Self {
        Self { endpoint, publisher_whitelist }
    }
}

fn get_publisher_id(request: &openrtb::BidRequest) -> String {
    // Check app publisher first, then site publisher
    if let Some(app) = &request.app {
        if let Some(pub_) = &app.publisher {
            if let Some(id) = &pub_.id {
                return id.clone();
            }
        }
    }
    if let Some(site) = &request.site {
        if let Some(pub_) = &site.publisher {
            if let Some(id) = &pub_.id {
                return id.clone();
            }
        }
    }
    "unknown".to_string()
}

fn is_msn(request: &openrtb::BidRequest) -> bool {
    let site_msn = request.site.as_ref()
        .and_then(|s| s.publisher.as_ref())
        .and_then(|p| p.domain.as_deref())
        .map(|d| d == "msn.com")
        .unwrap_or(false);
    let app_msn = request.app.as_ref()
        .and_then(|a| a.publisher.as_ref())
        .and_then(|p| p.domain.as_deref())
        .map(|d| d == "msn.com")
        .unwrap_or(false);
    site_msn || app_msn
}

impl Bidder for TripleliftNativeAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();

        // Check publisher whitelist (if non-empty)
        if !self.publisher_whitelist.is_empty() {
            let pub_id = get_publisher_id(request);
            if !self.publisher_whitelist.contains(&pub_id) {
                return (vec![], vec![BidderError::BadInput(
                    "Unsupported publisher for triplelift_native".to_string()
                )]);
            }
        }

        let msn = is_msn(request);
        let mut valid_imps: Vec<openrtb::Imp> = Vec::new();

        for imp in &request.imp {
            if imp.native.is_none() {
                errs.push(BidderError::BadInput("no native object specified".to_string()));
                continue;
            }

            let mut imp_copy = imp.clone();

            // Determine tag_id from ext
            let tag_id = imp.ext.as_ref().and_then(|e| {
                if msn {
                    // Use data.tag_code if available for MSN
                    e.get("data")
                        .and_then(|d| d.get("tag_code"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .or_else(|| {
                            e.get("bidder")
                                .and_then(|b| b.get("inventoryCode").or_else(|| b.get("inv_code")))
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string())
                        })
                } else {
                    e.get("bidder")
                        .and_then(|b| b.get("inventoryCode").or_else(|| b.get("inv_code")))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                }
            });

            if let Some(tid) = tag_id {
                imp_copy.tagid = Some(tid);
            }

            // Optional floor from ext.bidder.floor
            if let Some(floor) = imp.ext.as_ref()
                .and_then(|e| e.get("bidder"))
                .and_then(|b| b.get("floor"))
                .and_then(|v| v.as_f64())
            {
                imp_copy.bidfloor = Some(floor);
            }

            valid_imps.push(imp_copy);
        }

        if valid_imps.is_empty() {
            errs.push(BidderError::BadInput("No valid impressions for triplelift".to_string()));
            return (vec![], errs);
        }

        let mut req = request.clone();
        req.imp = valid_imps;

        let body = match serde_json::to_vec(&req) {
            Ok(b) => b,
            Err(e) => {
                errs.push(BidderError::BadInput(e.to_string()));
                return (vec![], errs);
            }
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        (
            vec![RequestData {
                method: "POST".to_string(),
                uri: self.endpoint.clone(),
                body,
                headers,
                imp_ids: get_imp_ids(&req.imp),
            }],
            errs,
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
        if let Err(e) = pbs_adapters::check_response_status(response.status_code) {
            return Err(vec![e]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let total_bids: usize = bid_resp.seatbid.iter().map(|sb| sb.bid.len()).sum();
        let mut result = BidderResponse::with_capacity(total_bids);

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                // TripleliftNative always returns native bids
                result.bids.push(TypedBid::new(bid, BidType::Native));
            }
        }

        Ok(result)
    }
}
