use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids, check_response_status};
use openrtb::BidResponse;
use openrtb_ext::BidType;

pub struct CadentApertureMxAdapter { pub endpoint: String }
impl CadentApertureMxAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

/// Determine bid type based on ad markup content (XML/VAST = video, else banner).
fn get_bid_type(adm: Option<&str>) -> BidType {
    if let Some(markup) = adm {
        let lower = markup.to_lowercase();
        if lower.contains("<?xml") || lower.contains("<vast") {
            return BidType::Video;
        }
    }
    BidType::Banner
}

/// Preprocess each imp: extract tagId from ext.bidder, set imp.tagid, validate bidfloor.
fn preprocess_imp(imp: &mut openrtb::Imp) -> Result<(), BidderError> {
    let bidder = imp.ext.as_ref()
        .and_then(|e| e.get("bidder"))
        .cloned()
        .unwrap_or(serde_json::Value::Null);

    let tag_id = bidder.get("tagid").or_else(|| bidder.get("tag_id"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if tag_id.is_empty() {
        return Err(BidderError::BadInput(format!("Ignoring imp id={}, no tagid present", imp.id)));
    }

    // Validate tagId is numeric
    tag_id.parse::<i64>().map_err(|_| BidderError::BadInput(
        format!("ignoring imp id={}, invalid tagid must be a String of numbers", imp.id)
    ))?;

    imp.tagid = Some(tag_id.to_string());

    // Set bidfloor from ext if present
    if let Some(bid_floor_str) = bidder.get("bidfloor").or_else(|| bidder.get("bid_floor"))
        .and_then(|v| v.as_str())
    {
        if let Ok(bf) = bid_floor_str.parse::<f64>() {
            if bf > 0.0 {
                imp.bidfloor = Some(bf);
                imp.bidfloorcur = Some("USD".to_string());
            }
        }
    }

    Ok(())
}

impl Bidder for CadentApertureMxAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::BadInput("No Imps in Bid Request".to_string())]);
        }

        let mut req = request.clone();
        let mut valid_imps = Vec::new();
        let mut errs = Vec::new();

        for mut imp in req.imp.into_iter() {
            match preprocess_imp(&mut imp) {
                Ok(()) => valid_imps.push(imp),
                Err(e) => errs.push(e),
            }
        }

        if valid_imps.is_empty() {
            return (vec![], errs);
        }

        req.imp = valid_imps;

        // Build URL with timestamp params
        let timeout = request.tmax.unwrap_or(1000);
        let url = format!("{}?t={}&ts=2060541160&src=pbserver", self.endpoint, timeout);

        let body = match serde_json::to_vec(&req) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(
                format!("Error in packaging request to JSON: {}", e)
            )]),
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
        if let Some(device) = &request.device {
            if let Some(ua) = &device.ua { if !ua.is_empty() { headers.insert("User-Agent".to_string(), ua.clone()); } }
            if let Some(ip) = &device.ip { if !ip.is_empty() { headers.insert("X-Forwarded-For".to_string(), ip.clone()); } }
            if let Some(lang) = &device.language { if !lang.is_empty() { headers.insert("Accept-Language".to_string(), lang.clone()); } }
        }
        if let Some(site) = &request.site {
            if let Some(page) = &site.page { if !page.is_empty() { headers.insert("Referer".to_string(), page.clone()); } }
        }

        (vec![RequestData {
            method: "POST".to_string(),
            uri: url,
            body,
            headers,
            imp_ids: get_imp_ids(&request.imp),
        }], errs)
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(
                format!("Unable to unpackage bid response. Error: {}", e)
            )])?;

        let mut result = BidderResponse::with_capacity(1);
        for mut sb in bid_resp.seatbid {
            for bid in sb.bid.iter_mut() {
                // cadent sets bid.id = bid.impid for tracking
                bid.impid = bid.id.clone();
                let bid_type = get_bid_type(bid.adm.as_deref());
                result.bids.push(TypedBid::new(bid.clone(), bid_type));
            }
        }
        Ok(result)
    }
}
