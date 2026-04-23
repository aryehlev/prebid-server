use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct TripleliftNativeAdapter {
    pub endpoint: String,
    /// Publisher IDs allowed to use this adapter.
    /// Checked as a set: if publisher ID is not present, requests are rejected.
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

/// Get the effective publisher ID from a request, mirroring Go's effectivePubID.
/// Checks publisher.ext.prebid.parentAccount first, then publisher.id, defaulting to "unknown".
fn effective_pub_id(publisher: Option<&openrtb::Publisher>) -> String {
    if let Some(pub_) = publisher {
        // Check ext.prebid.parentAccount
        if let Some(ext) = &pub_.ext {
            if let Some(parent_account) = ext
                .get("prebid")
                .and_then(|p| p.get("parentAccount"))
                .and_then(|v| v.as_str())
            {
                if !parent_account.is_empty() {
                    return parent_account.to_string();
                }
            }
        }
        if let Some(id) = &pub_.id {
            if !id.is_empty() {
                return id.clone();
            }
        }
    }
    "unknown".to_string()
}

/// Get the publisher from the request (app first, then site), mirroring Go's getPublisher.
fn get_publisher(request: &openrtb::BidRequest) -> Option<&openrtb::Publisher> {
    if let Some(app) = &request.app {
        return app.publisher.as_ref();
    }
    if let Some(site) = &request.site {
        return site.publisher.as_ref();
    }
    None
}

fn is_msn_in_site(request: &openrtb::BidRequest) -> bool {
    request.site.as_ref()
        .and_then(|s| s.publisher.as_ref())
        .and_then(|p| p.domain.as_deref())
        .map(|d| d == "msn.com")
        .unwrap_or(false)
}

fn is_msn_in_app(request: &openrtb::BidRequest) -> bool {
    request.app.as_ref()
        .and_then(|a| a.publisher.as_ref())
        .and_then(|p| p.domain.as_deref())
        .map(|d| d == "msn.com")
        .unwrap_or(false)
}

impl Bidder for TripleliftNativeAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();

        let msn = is_msn_in_site(request) || is_msn_in_app(request);
        let mut valid_imps: Vec<openrtb::Imp> = Vec::new();

        for imp in &request.imp {
            if imp.native.is_none() {
                errs.push(BidderError::BadInput("no native object specified".to_string()));
                continue;
            }

            let mut imp_copy = imp.clone();

            // Determine tagid from ext, matching Go's processImp logic:
            // If MSN and ext.data.tag_code is non-empty, use that; else use tlext.InvCode (bidder.inventoryCode)
            let tag_id = if msn {
                imp.ext.as_ref()
                    .and_then(|e| e.get("data"))
                    .and_then(|d| d.get("tag_code"))
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .or_else(|| {
                        imp.ext.as_ref()
                            .and_then(|e| e.get("bidder"))
                            .and_then(|b| b.get("inventoryCode").or_else(|| b.get("inv_code")))
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                    })
            } else {
                imp.ext.as_ref()
                    .and_then(|e| e.get("bidder"))
                    .and_then(|b| b.get("inventoryCode").or_else(|| b.get("inv_code")))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            };

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

        // Check publisher whitelist - matching Go behavior:
        // The whitelist map is always checked; if publisher ID is not in the map, reject.
        // An empty whitelist means NO publishers are whitelisted.
        let publisher = get_publisher(request);
        let publisher_id = effective_pub_id(publisher);
        if !self.publisher_whitelist.contains(&publisher_id) {
            return (vec![], vec![BidderError::BadInput(
                "Unsupported publisher for triplelift_native".to_string()
            )]);
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

        if response.status_code == 400 {
            return Err(vec![BidderError::BadInput(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info",
                response.status_code
            ))]);
        }

        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info",
                response.status_code
            ))]);
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
