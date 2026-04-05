use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use openrtb_ext::BidType;
use serde::Deserialize;

pub struct ConnectadAdapter {
    pub endpoint: String,
}

impl ConnectadAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

/// Extension from imp.ext.bidder for ConnectAd
#[derive(Debug, Default, Deserialize)]
struct ExtImpConnectAd {
    #[serde(rename = "siteId", default)]
    site_id: i64,
    #[serde(rename = "bidfloor", default)]
    bidfloor: f64,
}

/// Determine if the page URL uses HTTPS and return 1 if so, 0 otherwise.
fn get_secure_from_request(request: &openrtb::BidRequest) -> i32 {
    if let Some(site) = &request.site {
        if let Some(page) = &site.page {
            if page.starts_with("https://") {
                return 1;
            }
        }
    }
    0
}

/// Preprocess imps: extract site_id, set tagid/secure/bidfloor, clear ext.
/// Returns the processed imps and any errors.
fn preprocess(
    request: &openrtb::BidRequest,
) -> (Vec<openrtb::Imp>, Vec<BidderError>) {
    let secure = get_secure_from_request(request);
    let mut res_imps = Vec::new();
    let mut errors = Vec::new();

    for imp in &request.imp {
        // Extract ext.bidder
        let bidder_val = match imp.ext.as_ref().and_then(|e| e.get("bidder")) {
            Some(v) => v.clone(),
            None => {
                errors.push(BidderError::BadInput(format!(
                    "Impression id={} has an Error: missing bidder ext",
                    imp.id
                )));
                continue;
            }
        };

        let cad_ext: ExtImpConnectAd = match serde_json::from_value(bidder_val) {
            Ok(v) => v,
            Err(e) => {
                errors.push(BidderError::BadInput(format!(
                    "Impression id={}, has invalid Ext: {}",
                    imp.id, e
                )));
                continue;
            }
        };

        if cad_ext.site_id == 0 {
            errors.push(BidderError::BadInput(format!(
                "Impression id={}, has no siteId present",
                imp.id
            )));
            continue;
        }

        // Banner is required
        let banner = match &imp.banner {
            Some(b) => b.clone(),
            None => {
                errors.push(BidderError::BadInput(
                    "We need a Banner Object in the request".to_string(),
                ));
                continue;
            }
        };

        // If banner has no W/H, use first format entry
        let banner = if banner.w.is_none() && banner.h.is_none() {
            let formats = match banner.format.as_ref() {
                Some(f) if !f.is_empty() => f,
                _ => {
                    errors.push(BidderError::BadInput(
                        "At least one size is required".to_string(),
                    ));
                    continue;
                }
            };
            let first = &formats[0];
            let mut b = banner.clone();
            b.w = first.w;
            b.h = first.h;
            // Remove first format entry
            if let Some(ref mut fmts) = b.format {
                fmts.remove(0);
            }
            b
        } else {
            banner
        };

        let mut imp_copy = imp.clone();
        imp_copy.tagid = Some(cad_ext.site_id.to_string());
        imp_copy.secure = Some(secure);
        if cad_ext.bidfloor != 0.0 {
            imp_copy.bidfloor = Some(cad_ext.bidfloor);
            imp_copy.bidfloorcur = Some("USD".to_string());
        }
        imp_copy.banner = Some(banner);
        imp_copy.ext = None;

        res_imps.push(imp_copy);
    }

    (res_imps, errors)
}

impl Bidder for ConnectadAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let (processed_imps, mut errors) = preprocess(request);

        // If any preprocess errors occurred, return all errors (matching Go behavior).
        if !errors.is_empty() {
            errors.push(BidderError::BadInput("Error in preprocess of Imp".to_string()));
            return (vec![], errors);
        }

        if processed_imps.is_empty() {
            return (vec![], vec![BidderError::BadInput("Error in preprocess of Imp".to_string())]);
        }

        // Build a modified request with processed imps
        let mut req_copy = request.clone();
        let imp_ids = processed_imps.iter().map(|i| i.id.clone()).collect();
        req_copy.imp = processed_imps;

        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => {
                errors.push(BidderError::BadInput(format!(
                    "Error in packaging request to JSON: {}",
                    e
                )));
                return (vec![], errors);
            }
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        if let Some(device) = &request.device {
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
            let ip = device.ip.as_deref().filter(|s| !s.is_empty())
                .or_else(|| device.ipv6.as_deref().filter(|s| !s.is_empty()));
            if let Some(ip_val) = ip {
                headers.insert("X-Forwarded-For".to_string(), ip_val.to_string());
            }
            let dnt = device.dnt
                .map(|v| v.to_string())
                .unwrap_or_else(|| "0".to_string());
            headers.insert("DNT".to_string(), dnt);
        }

        (
            vec![RequestData {
                method: "POST".to_string(),
                uri: self.endpoint.clone(),
                body,
                headers,
                imp_ids,
            }],
            errors,
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
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Invalid Status Returned: {}. Run with request.debug = 1 for more info",
                response.status_code
            ))]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(format!(
                "Unable to unpackage bid response. Error: {}",
                e
            ))])?;

        let mut result = BidderResponse::with_capacity(bid_resp.seatbid.len());
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                // ConnectAd always returns banner bids.
                result.bids.push(TypedBid::new(bid, BidType::Banner));
            }
        }
        Ok(result)
    }
}
