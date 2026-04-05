use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb::BidResponse;
use openrtb_ext::BidType;

pub struct OperaadsAdapter { pub endpoint: String }
impl OperaadsAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

/// Build operaads imp ID: originId:opa:bidType
fn build_opera_imp_id(origin_id: &str, bid_type: &str) -> String {
    format!("{}:opa:{}", origin_id, bid_type)
}

/// Parse operaads imp ID back to (originId, bidType)
fn parse_opera_imp_id(imp_id: &str) -> (String, BidType) {
    let parts: Vec<&str> = imp_id.split(':').collect();
    if parts.len() < 2 {
        return (imp_id.to_string(), BidType::Banner);
    }
    let bid_type_str = parts[parts.len() - 1];
    let bid_type = match bid_type_str {
        "banner" => BidType::Banner,
        "video" => BidType::Video,
        "native" => BidType::Native,
        _ => BidType::Banner,
    };
    let origin = parts[..parts.len() - 2].join(":");
    (origin, bid_type)
}

/// Convert banner: ensure w/h are set from format if missing
fn convert_banner(banner: &mut openrtb::Banner) -> Result<(), BidderError> {
    let no_size = banner.w.is_none() || banner.h.is_none()
        || banner.w == Some(0) || banner.h == Some(0);
    if no_size {
        let first = banner.format.as_ref()
            .and_then(|f| f.first())
            .cloned();
        if let Some(fmt) = first {
            banner.w = fmt.w;
            banner.h = fmt.h;
        } else {
            return Err(BidderError::BadInput("Size information missing for banner".to_string()));
        }
    }
    Ok(())
}

/// Convert native: if native.request is non-empty and not already wrapped in {"native": ...},
/// wrap it (matching Go's convertImpression logic).
fn convert_native(native: &mut openrtb::Native) -> Result<(), BidderError> {
    let req = match &native.request {
        Some(r) if !r.is_empty() => r.clone(),
        _ => return Ok(()),
    };
    // Parse the request JSON
    let v: serde_json::Value = serde_json::from_str(&req)
        .map_err(|e| BidderError::BadInput(e.to_string()))?;
    // Check if already has "native" key
    if v.get("native").is_none() {
        let wrapped = serde_json::json!({ "native": v });
        native.request = Some(serde_json::to_string(&wrapped)
            .map_err(|e| BidderError::BadInput(e.to_string()))?);
    }
    Ok(())
}

/// Build a single per-format request
fn build_format_request(
    request: &openrtb::BidRequest,
    mut imp: openrtb::Imp,
    headers: HashMap<String, String>,
    endpoint: &str,
    bid_type: &str,
) -> Result<RequestData, BidderError> {
    imp.id = build_opera_imp_id(&imp.id, bid_type);

    // Clear other format fields and apply conversions
    match bid_type {
        "banner" => {
            imp.video = None;
            imp.native = None;
            if let Some(banner) = &mut imp.banner {
                convert_banner(banner)?;
            }
        }
        "video" => {
            imp.banner = None;
            imp.native = None;
        }
        "native" => {
            imp.banner = None;
            imp.video = None;
            if let Some(native) = &mut imp.native {
                convert_native(native)?;
            }
        }
        _ => {}
    }

    let mut req_copy = request.clone();
    req_copy.imp = vec![imp];
    let imp_ids = get_imp_ids(&req_copy.imp);

    let body = serde_json::to_vec(&req_copy)
        .map_err(|e| BidderError::BadInput(e.to_string()))?;

    Ok(RequestData {
        method: "POST".to_string(),
        uri: endpoint.to_string(),
        body,
        headers,
        imp_ids,
    })
}

impl Bidder for OperaadsAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        // Validate device OS is present (matching Go's checkRequest)
        let device_os_ok = request.device.as_ref()
            .and_then(|d| d.os.as_ref())
            .map(|os| !os.is_empty())
            .unwrap_or(false);
        if !device_os_ok {
            return (vec![], vec![BidderError::BadInput(
                "Impression is missing device OS information".to_string()
            )]);
        }

        let mut errs = Vec::new();
        let mut requests = Vec::new();

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        for imp in &request.imp {
            // Get endpoint URL from imp.ext.bidder publisherId and endpointId
            let bidder = imp.ext.as_ref().and_then(|e| e.get("bidder"));
            let publisher_id = bidder.and_then(|b| b.get("publisherId")).and_then(|v| v.as_str()).unwrap_or("");
            let endpoint_id = bidder.and_then(|b| b.get("endpointId")).and_then(|v| v.as_str()).unwrap_or("");
            let placement_id = bidder.and_then(|b| b.get("placementId")).and_then(|v| v.as_str()).unwrap_or("").to_string();

            // Build endpoint URL: replace template params
            let endpoint = self.endpoint
                .replace("{{.PublisherID}}", publisher_id)
                .replace("{{.AccountID}}", endpoint_id);

            let mut imp_copy = imp.clone();
            imp_copy.tagid = Some(placement_id);

            // Build per-format requests: native, video, banner (Go order)
            if imp.native.is_some() {
                match build_format_request(request, imp_copy.clone(), headers.clone(), &endpoint, "native") {
                    Ok(rd) => requests.push(rd),
                    Err(e) => errs.push(e),
                }
            }
            if imp.video.is_some() {
                match build_format_request(request, imp_copy.clone(), headers.clone(), &endpoint, "video") {
                    Ok(rd) => requests.push(rd),
                    Err(e) => errs.push(e),
                }
            }
            if imp.banner.is_some() {
                match build_format_request(request, imp_copy.clone(), headers.clone(), &endpoint, "banner") {
                    Ok(rd) => requests.push(rd),
                    Err(e) => errs.push(e),
                }
            }
        }

        (requests, errs)
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        // Use explicit status code checks matching Go's MakeBids
        if response.status_code == 400 {
            return Err(vec![BidderError::BadInput(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info", response.status_code
            ))]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info", response.status_code
            ))]);
        }
        let bid_resp: BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(5);
        for sb in bid_resp.seatbid {
            for mut bid in sb.bid {
                // Only include bids with non-zero price (matching Go)
                if bid.price == 0.0 { continue; }
                let (origin_id, bid_type) = parse_opera_imp_id(&bid.impid);
                bid.impid = origin_id;
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        Ok(result)
    }
}
