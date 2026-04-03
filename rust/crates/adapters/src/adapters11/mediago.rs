use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_bid_type_from_imp, get_imp_ids, check_response_status};
use openrtb::BidResponse;
use openrtb_ext::BidType;

pub struct MediagoAdapter { pub endpoint: String }
impl MediagoAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

fn get_region_host(region: &str) -> &'static str {
    match region {
        "APAC" => "jp",
        "EU" => "eu",
        "US" | _ => "us",
    }
}

/// Get the token and region from the first imp's ext.bidder
fn get_token_and_region(request: &openrtb::BidRequest) -> Result<(String, String), BidderError> {
    let imp = request.imp.first()
        .ok_or_else(|| BidderError::BadInput("no impressions".to_string()))?;
    let bidder = imp.ext.as_ref()
        .and_then(|e| e.get("bidder"))
        .ok_or_else(|| BidderError::BadInput("mediago token not found".to_string()))?;
    let token = bidder.get("token")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if token.is_empty() {
        return Err(BidderError::BadInput("mediago token not found".to_string()));
    }
    let region = bidder.get("region")
        .and_then(|v| v.as_str())
        .unwrap_or("US")
        .to_string();
    Ok((token, region))
}

fn build_url(endpoint_template: &str, token: &str, region: &str) -> String {
    let host = get_region_host(region);
    endpoint_template
        .replace("{{.AccountID}}", token)
        .replace("{{.Host}}", host)
}

fn preprocess_banner(request: &mut openrtb::BidRequest) {
    for imp in &mut request.imp {
        if let Some(banner) = &mut imp.banner {
            let no_size = banner.w.is_none() || banner.h.is_none()
                || banner.w == Some(0) || banner.h == Some(0);
            if no_size {
                if let Some(formats) = &banner.format {
                    if let Some(first) = formats.first() {
                        banner.w = first.w;
                        banner.h = first.h;
                    }
                }
            }
        }
    }
}

impl Bidder for MediagoAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let (token, region) = match get_token_and_region(request) {
            Ok(v) => v,
            Err(e) => return (vec![], vec![e]),
        };

        let endpoint = build_url(&self.endpoint, &token, &region);

        let mut req_copy = request.clone();
        preprocess_banner(&mut req_copy);

        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
        headers.insert("x-openrtb-version".to_string(), "2.5".to_string());

        let imp_ids = get_imp_ids(&req_copy.imp);
        (vec![RequestData {
            method: "POST".to_string(),
            uri: endpoint,
            body,
            headers,
            imp_ids,
        }], vec![])
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(5);
        if let Some(cur) = &bid_resp.cur {
            if !cur.is_empty() { result.currency = cur.clone(); }
        }
        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                // Determine bid type from mtype (in ext) or imp type
                let mtype = bid.ext.as_ref()
                    .and_then(|e| e.get("mtype"))
                    .and_then(|v| v.as_u64());
                let bid_type = match mtype {
                    Some(1) => BidType::Banner,
                    Some(4) => BidType::Native,
                    _ => {
                        // fallback to imp type
                        internal.imp.iter().find(|i| i.id == bid.impid)
                            .map(get_bid_type_from_imp)
                            .unwrap_or(BidType::Banner)
                    }
                };
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        if !errs.is_empty() { return Err(errs); }
        Ok(result)
    }
}
