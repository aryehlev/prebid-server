use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_bid_type_from_imp, get_imp_ids};
use serde::Deserialize;

pub struct YandexAdapter { pub endpoint: String }
impl YandexAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

#[derive(Debug, Default, Deserialize)]
struct ExtImpYandex {
    #[serde(rename = "placementId", default)]
    placement_id: String,
    #[serde(rename = "pageId", default)]
    page_id: i64,
    #[serde(rename = "impId", default)]
    imp_id: i64,
}

fn resolve_placement(ext: &ExtImpYandex) -> Result<(String, String), BidderError> {
    if ext.placement_id.is_empty() {
        let page_id = ext.page_id.to_string();
        let imp_id = ext.imp_id.to_string();
        return Ok((page_id, imp_id));
    }
    // Split on '-' and take last two numeric parts
    let parts: Vec<&str> = ext.placement_id.split('-').collect();
    let numeric: Vec<&str> = parts.iter().filter(|p| p.parse::<i64>().is_ok()).copied().collect();
    if numeric.len() < 2 {
        return Err(BidderError::BadInput(format!(
            "invalid placement id, it must contain two parts: {}", ext.placement_id
        )));
    }
    let n = numeric.len();
    Ok((numeric[n - 2].to_string(), numeric[n - 1].to_string()))
}

impl Bidder for YandexAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();
        let mut errs = Vec::new();

        let referer = request.site.as_ref().and_then(|s| {
            if s.page.as_ref().map_or(false, |p| !p.is_empty()) {
                s.page.clone()
            } else {
                s.domain.clone()
            }
        }).unwrap_or_default();

        let currency = request.cur.as_ref().and_then(|c| c.first().cloned()).unwrap_or_default();

        for imp in &request.imp {
            let bidder_val = match imp.ext.as_ref().and_then(|e| e.get("bidder")).cloned() {
                Some(v) => v,
                None => {
                    errs.push(BidderError::BadInput(format!("imp {}: missing bidder ext", imp.id)));
                    continue;
                }
            };
            let yandex_ext: ExtImpYandex = match serde_json::from_value(bidder_val) {
                Ok(e) => e,
                Err(e) => {
                    errs.push(BidderError::BadInput(format!("imp {}: unable to unmarshal ext.bidder: {}", imp.id, e)));
                    continue;
                }
            };

            let (page_id, imp_id_str) = match resolve_placement(&yandex_ext) {
                Ok(p) => p,
                Err(e) => {
                    errs.push(e);
                    continue;
                }
            };

            // Build URL: replace {{.PageID}} macro and add query params
            let base_url = self.endpoint.replace("{{.PageID}}", &page_id);
            let mut url = base_url;
            // Add query params
            let mut params = Vec::new();
            if !referer.is_empty() {
                params.push(format!("target-ref={}", urlencoding(&referer)));
            }
            if !currency.is_empty() {
                params.push(format!("ssp-cur={}", urlencoding(&currency)));
            }
            if !imp_id_str.is_empty() {
                params.push(format!("imp-id={}", urlencoding(&imp_id_str)));
            }
            if !params.is_empty() {
                if url.contains('?') {
                    url.push('&');
                } else {
                    url.push('?');
                }
                url.push_str(&params.join("&"));
            }

            let mut single_req = request.clone();
            single_req.imp = vec![imp.clone()];

            // Add headers
            let mut headers = HashMap::new();
            headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
            headers.insert("Accept".to_string(), "application/json".to_string());
            headers.insert("X-OpenRTB-Version".to_string(), "2.5".to_string());
            if let Some(device) = &request.device {
                if let Some(ua) = &device.ua {
                    if !ua.is_empty() {
                        headers.insert("User-Agent".to_string(), ua.clone());
                    }
                }
                if let Some(ip) = &device.ip {
                    if !ip.is_empty() {
                        headers.insert("X-Forwarded-For".to_string(), ip.clone());
                        headers.insert("X-Real-Ip".to_string(), ip.clone());
                    }
                }
                if let Some(lang) = &device.language {
                    if !lang.is_empty() {
                        headers.insert("Accept-Language".to_string(), lang.clone());
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

            let imp_ids = get_imp_ids(&single_req.imp);
            let body = match serde_json::to_vec(&single_req) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(BidderError::BadInput(e.to_string()));
                    continue;
                }
            };

            requests.push(RequestData { method: "POST".to_string(), uri: url, body, headers, imp_ids });
        }

        (requests, errs)
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(5);
        let mut errs = Vec::new();

        // Build imp map
        let imp_map: HashMap<&str, &openrtb::Imp> = internal.imp.iter().map(|i| (i.id.as_str(), i)).collect();

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                match imp_map.get(bid.impid.as_str()) {
                    Some(imp) => {
                        let bid_type = get_bid_type_from_imp(imp);
                        result.bids.push(TypedBid::new(bid, bid_type));
                    }
                    None => {
                        errs.push(BidderError::BadInput(format!(
                            "Invalid bid imp ID #{} does not match any imp IDs from the original bid request",
                            bid.impid
                        )));
                    }
                }
            }
        }
        Ok(result)
    }
}

fn urlencoding(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push('%');
                out.push_str(&format!("{:02X}", b));
            }
        }
    }
    out
}
