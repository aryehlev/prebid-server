use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;
use serde_json::Value;

pub struct BetweenAdapter { pub endpoint: String }
impl BetweenAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

#[derive(Deserialize)]
struct ExtImpBidder { bidder: Value }

#[derive(Deserialize)]
struct ExtImpBetween {
    #[serde(default)]
    host: String,
    #[serde(default)]
    publisher_id: String,
}

impl Bidder for BetweenAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::BadInput("No valid Imps in Bid Request".to_string())]);
        }

        // Get between ext from first valid imp
        let mut between_ext: Option<ExtImpBetween> = None;
        let mut errs = Vec::new();

        let mut processed_imps = Vec::new();

        // Check if site is https for secure flag
        let secure = request.site.as_ref()
            .and_then(|s| s.page.as_ref())
            .and_then(|page| if page.starts_with("https://") { Some(1i32) } else { Some(0i32) })
            .unwrap_or(0);

        for imp in &request.imp {
            let ext_val = match &imp.ext {
                Some(v) => v.clone(),
                None => {
                    errs.push(BidderError::BadInput(format!("ignoring imp id={}, invalid BidderExt", imp.id)));
                    continue;
                }
            };

            let bidder_ext: ExtImpBidder = match serde_json::from_value(ext_val) {
                Ok(v) => v,
                Err(_) => {
                    errs.push(BidderError::BadInput(format!("ignoring imp id={}, invalid BidderExt", imp.id)));
                    continue;
                }
            };

            let ext: ExtImpBetween = match serde_json::from_value(bidder_ext.bidder) {
                Ok(v) => v,
                Err(_) => {
                    errs.push(BidderError::BadInput(format!("ignoring imp id={}, invalid ImpExt", imp.id)));
                    continue;
                }
            };

            if ext.host.is_empty() {
                errs.push(BidderError::BadInput(r#"required BetweenSSP parameter "host" is missing"#.to_string()));
                continue;
            }
            if ext.publisher_id.is_empty() {
                errs.push(BidderError::BadInput(r#"required BetweenSSP parameter "publisher_id" is missing"#.to_string()));
                continue;
            }

            // Build imp with banner size from format if needed
            let mut new_imp = imp.clone();
            new_imp.secure = Some(secure);

            if let Some(banner) = &mut new_imp.banner {
                if banner.w.is_none() && banner.h.is_none() {
                    if let Some(formats) = &banner.format {
                        if !formats.is_empty() {
                            let first = &formats[0];
                            let w = first.w;
                            let h = first.h;
                            let rest = formats[1..].to_vec();
                            banner.format = if rest.is_empty() { None } else { Some(rest) };
                            banner.w = w;
                            banner.h = h;
                        } else {
                            errs.push(BidderError::BadInput("Need at least one size to build request".to_string()));
                            continue;
                        }
                    } else {
                        errs.push(BidderError::BadInput("Request needs to include a Banner object".to_string()));
                        continue;
                    }
                }
            } else {
                errs.push(BidderError::BadInput("Request needs to include a Banner object".to_string()));
                continue;
            }

            if between_ext.is_none() {
                between_ext = Some(ext);
            }

            processed_imps.push(new_imp);
        }

        let ext = match between_ext {
            Some(e) => e,
            None => return (vec![], errs),
        };

        let url = self.endpoint
            .replace("{{.Host}}", &ext.host)
            .replace("{{.PublisherID}}", &ext.publisher_id);

        let mut req = request.clone();
        req.imp = processed_imps;

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        if let Some(device) = &request.device {
            if let Some(ua) = &device.ua { if !ua.is_empty() { headers.insert("User-Agent".to_string(), ua.clone()); } }
            if let Some(ip) = &device.ip { if !ip.is_empty() { headers.insert("X-Forwarded-For".to_string(), ip.clone()); } }
            if let Some(lang) = &device.language { if !lang.is_empty() { headers.insert("Accept-Language".to_string(), lang.clone()); } }
            if let Some(dnt) = device.dnt { headers.insert("DNT".to_string(), dnt.to_string()); }
        }
        if let Some(site) = &request.site {
            if let Some(page) = &site.page { if !page.is_empty() { headers.insert("Referer".to_string(), page.clone()); } }
        }

        let body = match serde_json::to_vec(&req) {
            Ok(b) => b,
            Err(_) => return (vec![], vec![BidderError::BadInput("Error in packaging request to JSON".to_string())]),
        };

        (vec![RequestData {
            method: "POST".to_string(),
            uri: url,
            body,
            headers,
            imp_ids: get_imp_ids(&req.imp),
        }], errs)
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(
                format!("Invalid Status Returned: {}. Run with request.debug = 1 for more info", response.status_code)
            )]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(format!("Unable to unpackage bid response. Error {}", e))])?;

        let mut result = BidderResponse::with_capacity(1);

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                result.bids.push(TypedBid::new(bid, BidType::Banner));
            }
        }

        Ok(result)
    }
}
