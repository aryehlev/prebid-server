use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids, get_bid_type_from_mtype};
use serde::Deserialize;

pub struct RediadsAdapter { pub endpoint: String }
impl RediadsAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

#[derive(Debug, Deserialize, Default)]
struct ExtImpRediads {
    #[serde(rename = "account_id", default)]
    account_id: String,
    #[serde(rename = "slot", default)]
    slot: String,
    #[serde(rename = "endpoint", default)]
    endpoint: String,
}

impl Bidder for RediadsAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut account_id = String::new();
        let mut endpoint_subdomain = String::new();
        let mut req_copy = request.clone();

        for imp in req_copy.imp.iter_mut() {
            // Extract bidder ext
            let ext_val = match &imp.ext {
                Some(e) => e.clone(),
                None => {
                    errs.push(BidderError::BadInput(format!("Invalid Ext format in impression {}", imp.id)));
                    continue;
                }
            };

            let bidder_val = match ext_val.get("bidder") {
                Some(v) => v.clone(),
                None => {
                    errs.push(BidderError::BadInput(format!("Invalid Ext format in impression {}", imp.id)));
                    continue;
                }
            };

            // Remove prebid and bidder from ext
            let mut new_ext = ext_val.clone();
            if let Some(obj) = new_ext.as_object_mut() {
                obj.remove("prebid");
                obj.remove("bidder");
            }
            if new_ext.as_object().map_or(false, |m| m.is_empty()) {
                imp.ext = None;
            } else {
                imp.ext = Some(new_ext);
            }

            let rediads_ext: ExtImpRediads = match serde_json::from_value(bidder_val) {
                Ok(e) => e,
                Err(_) => {
                    errs.push(BidderError::BadInput(format!("Invalid bidder params in impression {}", imp.id)));
                    continue;
                }
            };

            account_id = rediads_ext.account_id;
            endpoint_subdomain = rediads_ext.endpoint;

            if !rediads_ext.slot.is_empty() {
                imp.tagid = Some(rediads_ext.slot);
            }
        }

        // Update site or app publisher ID
        if let Some(site) = &req_copy.site {
            let mut site_copy = site.clone();
            let mut publisher = site_copy.publisher.clone().unwrap_or_default();
            publisher.id = Some(account_id.clone());
            site_copy.publisher = Some(publisher);
            req_copy.site = Some(site_copy);
        } else if let Some(app) = &req_copy.app {
            let mut app_copy = app.clone();
            let mut publisher = app_copy.publisher.clone().unwrap_or_default();
            publisher.id = Some(account_id.clone());
            app_copy.publisher = Some(publisher);
            req_copy.app = Some(app_copy);
        }

        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        // Build final endpoint URL - replace first subdomain if endpoint is specified
        let final_endpoint = if !endpoint_subdomain.is_empty() {
            // Parse the URL to extract and replace the host
            if let Some(after_scheme) = self.endpoint.find("://") {
                let rest = &self.endpoint[after_scheme + 3..];
                let host_end = rest.find('/').unwrap_or(rest.len());
                let host = &rest[..host_end];
                let parts: Vec<&str> = host.splitn(2, '.').collect();
                if parts.len() == 2 {
                    let new_host = format!("{}.{}", endpoint_subdomain, parts[1]);
                    self.endpoint.replacen(host, &new_host, 1)
                } else {
                    self.endpoint.clone()
                }
            } else {
                self.endpoint.clone()
            }
        } else {
            self.endpoint.clone()
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        (vec![RequestData {
            method: "POST".to_string(),
            uri: final_endpoint,
            body,
            headers,
            imp_ids: get_imp_ids(&req_copy.imp),
        }], errs)
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(5);
        if let Some(cur) = &bid_resp.cur { if !cur.is_empty() { result.currency = cur.clone(); } }
        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let mtype = bid.mtype.unwrap_or(0);
                if mtype == 0 {
                    errs.push(BidderError::BadServerResponse(
                        format!("could not define media type for impression: {}", bid.impid)
                    ));
                    continue;
                }
                result.bids.push(TypedBid::new(bid, get_bid_type_from_mtype(mtype)));
            }
        }
        if !errs.is_empty() && result.bids.is_empty() { return Err(errs); }
        Ok(result)
    }
}
