use std::collections::HashMap;
use pbs_adapters::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct YandexAdapter {
    pub endpoint: String,
}

impl YandexAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }

    /// Build URL from endpoint template.
    /// The Go adapter uses {{.PageID}} macro; we replace it here.
    fn resolve_url(&self, page_id: &str, imp_id: &str, referer: &str, currency: &str) -> String {
        let base = self.endpoint.replace("{{.PageID}}", page_id);
        let mut url = base;

        // Append query params
        let mut query_parts = Vec::new();
        if !imp_id.is_empty() {
            query_parts.push(format!("imp-id={}", imp_id));
        }
        if !referer.is_empty() {
            query_parts.push(format!("target-ref={}", encode_query_value(referer)));
        }
        if !currency.is_empty() {
            query_parts.push(format!("ssp-cur={}", currency));
        }

        if !query_parts.is_empty() {
            let sep = if url.contains('?') { "&" } else { "?" };
            url = format!("{}{}{}", url, sep, query_parts.join("&"));
        }

        url
    }
}

fn encode_query_value(s: &str) -> String {
    // Simple percent-encoding for query parameter values
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9'
            | b'-' | b'_' | b'.' | b'~' | b':' | b'/' | b'?' | b'#'
            | b'[' | b']' | b'@' | b'!' | b'$' | b'&' | b'\'' | b'('
            | b')' | b'*' | b'+' | b',' | b';' | b'=' => out.push(b as char),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

fn get_bid_type(imp: &openrtb::Imp) -> Result<BidType, BidderError> {
    if imp.video.is_some() {
        Ok(BidType::Video)
    } else if imp.native.is_some() {
        Ok(BidType::Native)
    } else if imp.banner.is_some() {
        Ok(BidType::Banner)
    } else {
        Err(BidderError::BadInput(format!(
            "Processing an invalid impression; cannot resolve impression type for imp #{}",
            imp.id
        )))
    }
}

impl Bidder for YandexAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut requests = Vec::new();

        let referer = request.site.as_ref()
            .and_then(|s| {
                if s.page.as_deref().unwrap_or("").is_empty() {
                    s.domain.as_deref()
                } else {
                    s.page.as_deref()
                }
            })
            .unwrap_or("");

        let currency = request.cur.first().map(|s| s.as_str()).unwrap_or("");

        for imp in &request.imp {
            // Get placement ID from bidder ext
            let bidder_ext = match imp.ext.as_ref().and_then(|e| e.get("bidder")) {
                Some(b) => b.clone(),
                None => {
                    errs.push(BidderError::BadInput(format!("imp {}: unable to unmarshal ext", imp.id)));
                    continue;
                }
            };

            // Resolve page_id and imp_id_part
            let (page_id, imp_id_part) = if let Some(placement_id) = bidder_ext.get("placementId").and_then(|v| v.as_str()) {
                // Split on "-", get last two numeric parts
                let numeric_parts: Vec<&str> = placement_id.split('-')
                    .filter(|p| p.parse::<i64>().is_ok())
                    .collect();
                if numeric_parts.len() < 2 {
                    errs.push(BidderError::BadInput(format!(
                        "invalid placement id, it must contain two parts: {}", placement_id
                    )));
                    continue;
                }
                let page = numeric_parts[numeric_parts.len() - 2].to_string();
                let imp_part = numeric_parts[numeric_parts.len() - 1].to_string();
                (page, imp_part)
            } else {
                let page = bidder_ext.get("pageId")
                    .and_then(|v| v.as_i64())
                    .map(|n| n.to_string())
                    .unwrap_or_default();
                let imp_part = bidder_ext.get("impId")
                    .and_then(|v| v.as_i64())
                    .map(|n| n.to_string())
                    .unwrap_or_default();
                (page, imp_part)
            };

            // Modify imp
            let mut imp = imp.clone();
            imp.display_manager = Some("prebid.go".to_string());
            imp.display_manager_ver = Some("1.1".to_string());

            // Modify banner: ensure w/h set from first format
            if let Some(banner) = imp.banner.as_mut() {
                let needs_size = banner.w.map(|w| w == 0).unwrap_or(true)
                    || banner.h.map(|h| h == 0).unwrap_or(true);
                if needs_size {
                    if banner.format.is_empty() {
                        errs.push(BidderError::BadInput("Invalid size provided for Banner".to_string()));
                        continue;
                    }
                    let first = &banner.format[0];
                    banner.w = Some(first.w);
                    banner.h = Some(first.h);
                }
            }

            // Modify video: ensure w/h set, set defaults
            if let Some(video) = imp.video.as_mut() {
                let w_ok = video.w.map(|w| w > 0).unwrap_or(false);
                let h_ok = video.h.map(|h| h > 0).unwrap_or(false);
                if !w_ok || !h_ok {
                    errs.push(BidderError::BadInput("Invalid size provided for Video".to_string()));
                    continue;
                }
                if video.minduration.unwrap_or(0) == 0 {
                    video.minduration = Some(1);
                }
                if video.maxduration.unwrap_or(0) == 0 {
                    video.maxduration = Some(120);
                }
                if video.protocols.is_empty() {
                    video.protocols = vec![3];
                }
            }

            // Must have at least one supported type
            if imp.banner.is_none() && imp.video.is_none() && imp.native.is_none() {
                errs.push(BidderError::BadInput(format!(
                    "Unsupported format. Yandex only supports banner, video, and native types. Ignoring imp id #{}", imp.id
                )));
                continue;
            }

            let url = self.resolve_url(&page_id, &imp_id_part, referer, currency);

            let mut split_req = request.clone();
            split_req.imp = vec![imp];

            let mut headers = HashMap::new();
            if let Some(device) = request.device.as_ref() {
                if let Some(site) = request.site.as_ref() {
                    if let Some(page) = site.page.as_deref() {
                        if !page.is_empty() { headers.insert("Referer".to_string(), page.to_string()); }
                    }
                    if let Some(lang) = device.language.as_deref() {
                        if !lang.is_empty() { headers.insert("Accept-Language".to_string(), lang.to_string()); }
                    }
                    if let Some(ua) = device.ua.as_deref() {
                        if !ua.is_empty() { headers.insert("User-Agent".to_string(), ua.to_string()); }
                    }
                    if let Some(ip) = device.ip.as_deref() {
                        if !ip.is_empty() {
                            headers.insert("X-Forwarded-For".to_string(), ip.to_string());
                            headers.insert("X-Real-Ip".to_string(), ip.to_string());
                        }
                    }
                }
                headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
                headers.insert("Accept".to_string(), "application/json".to_string());
                headers.insert("X-OpenRTB-Version".to_string(), "2.5".to_string());
            }

            let body = match serde_json::to_vec(&split_req) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(BidderError::BadInput(e.to_string()));
                    continue;
                }
            };

            requests.push(RequestData {
                method: "POST".to_string(),
                uri: url,
                body,
                headers,
                imp_ids: get_imp_ids(&split_req.imp),
            });
        }

        (requests, errs)
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
        if let Err(e) = pbs_adapters::check_response_status(response.status_code) {
            return Err(vec![e]);
        }

        let bid_response: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(format!("Bad server response: {}", e))])?;

        let mut result = BidderResponse::with_capacity(internal.imp.len());
        let mut errs = Vec::new();

        let imp_map: HashMap<&str, &openrtb::Imp> = internal.imp.iter()
            .map(|i| (i.id.as_str(), i))
            .collect();

        for sb in bid_response.seatbid {
            for bid in sb.bid {
                let imp = match imp_map.get(bid.impid.as_str()) {
                    Some(i) => *i,
                    None => {
                        errs.push(BidderError::BadInput(format!(
                            "Invalid bid imp ID #{} does not match any imp IDs from the original bid request",
                            bid.impid
                        )));
                        continue;
                    }
                };

                let bid_type = match get_bid_type(imp) {
                    Ok(t) => t,
                    Err(e) => {
                        errs.push(e);
                        continue;
                    }
                };

                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }

        if errs.is_empty() {
            Ok(result)
        } else {
            Ok(result)
        }
    }
}
