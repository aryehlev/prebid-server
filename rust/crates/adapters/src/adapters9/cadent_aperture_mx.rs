use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb::BidResponse;
use openrtb_ext::BidType;
use serde::Deserialize;
use serde_json::Value;

pub struct CadentApertureMxAdapter {
    pub endpoint: String,
}

impl CadentApertureMxAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

#[derive(Deserialize)]
struct ExtImpBidder {
    bidder: Value,
}

#[derive(Deserialize)]
struct ExtImpCadentApertureMX {
    tagid: String,
    #[serde(default)]
    bidfloor: String,
}

/// Build endpoint URL with timeout and timestamp params.
fn build_endpoint(endpoint: &str, tmax: Option<i64>) -> String {
    let timeout = tmax.unwrap_or(1000);
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{}?t={}&ts={}&src=pbserver", endpoint, timeout, ts)
}

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

/// Unpack and validate the cadent imp extension fields.
fn unpack_imp_ext(imp: &openrtb::Imp) -> Result<ExtImpCadentApertureMX, BidderError> {
    let ext_val = imp.ext.as_ref()
        .ok_or_else(|| BidderError::BadInput(format!("ignoring imp id={}, invalid ImpExt", imp.id)))?;

    let bidder_ext: ExtImpBidder = serde_json::from_value(ext_val.clone())
        .map_err(|e| BidderError::BadInput(e.to_string()))?;

    let cadent_ext: ExtImpCadentApertureMX = serde_json::from_value(bidder_ext.bidder)
        .map_err(|_| BidderError::BadInput(format!("ignoring imp id={}, invalid ImpExt", imp.id)))?;

    if cadent_ext.tagid.is_empty() {
        return Err(BidderError::BadInput(format!("Ignoring imp id={}, no tagid present", imp.id)));
    }

    cadent_ext.tagid.parse::<i64>().map_err(|_| BidderError::BadInput(
        format!("ignoring imp id={}, invalid tagid must be a String of numbers", imp.id)
    ))?;

    Ok(cadent_ext)
}

/// Add cadent required properties to imp.
fn add_imp_props(imp: &mut openrtb::Imp, secure: i32, cadent_ext: &ExtImpCadentApertureMX) {
    imp.tagid = Some(cadent_ext.tagid.clone());
    imp.secure = Some(secure);

    if !cadent_ext.bidfloor.is_empty() {
        if let Ok(bf) = cadent_ext.bidfloor.parse::<f64>() {
            if bf > 0.0 {
                imp.bidfloor = Some(bf);
                imp.bidfloorcur = Some("USD".to_string());
            }
        }
    }
}

/// Validate and possibly adjust banner object.
fn build_imp_banner(imp: &mut openrtb::Imp) -> Result<(), BidderError> {
    let banner = imp.banner.as_mut()
        .ok_or_else(|| BidderError::BadInput("Request needs to include a Banner object".to_string()))?;

    if banner.w.is_none() && banner.h.is_none() {
        let formats = banner.format.as_mut()
            .filter(|f| !f.is_empty())
            .ok_or_else(|| BidderError::BadInput("Need at least one size to build request".to_string()))?;

        let first = formats.remove(0);
        banner.w = first.w;
        banner.h = first.h;
    }

    Ok(())
}

/// Validate video object. Not supporting VAST protocol 7 (VAST 4.0).
fn build_imp_video(imp: &mut openrtb::Imp) -> Result<(), BidderError> {
    let video = imp.video.as_mut()
        .expect("build_imp_video called without video");

    let mimes_empty = video.mimes.as_ref().map_or(true, |m| m.is_empty());
    if mimes_empty {
        return Err(BidderError::BadInput("Video: missing required field mimes".to_string()));
    }

    let no_size = (video.h.is_none() || video.h == Some(0))
        && (video.w.is_none() || video.w == Some(0));
    if no_size {
        return Err(BidderError::BadInput("Video: Need at least one size to build request".to_string()));
    }

    // Remove VAST 4.0 protocol (value 7) - not supported
    if let Some(protocols) = video.protocols.as_mut() {
        protocols.retain(|&p| p != 7);
    }

    Ok(())
}

/// Preprocess the bid request: validate imps, set tagid, secure, bidfloor, validate banner/video.
fn preprocess(request: &mut openrtb::BidRequest) -> Vec<BidderError> {
    let mut errors = Vec::new();
    let mut res_imps = Vec::new();

    // Determine secure flag from domain URL scheme
    let domain = if let Some(site) = &request.site {
        site.page.clone().unwrap_or_default()
    } else if let Some(app) = &request.app {
        app.domain.clone()
            .or_else(|| app.storeurl.clone())
            .unwrap_or_default()
    } else {
        String::new()
    };

    let secure = if domain.starts_with("https://") { 1i32 } else { 0i32 };

    let imps = std::mem::take(&mut request.imp);
    for mut imp in imps {
        let cadent_ext = match unpack_imp_ext(&imp) {
            Ok(e) => e,
            Err(e) => { errors.push(e); continue; }
        };

        add_imp_props(&mut imp, secure, &cadent_ext);

        if imp.video.is_some() {
            if let Err(e) = build_imp_video(&mut imp) {
                errors.push(e);
                continue;
            }
        } else if let Err(e) = build_imp_banner(&mut imp) {
            errors.push(e);
            continue;
        }

        res_imps.push(imp);
    }

    request.imp = res_imps;
    errors
}

impl Bidder for CadentApertureMxAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::BadInput("No Imps in Bid Request".to_string())]);
        }

        let mut req = request.clone();
        let preprocess_errs = preprocess(&mut req);

        if !preprocess_errs.is_empty() {
            let first_msg = preprocess_errs.first().map(|e| e.to_string()).unwrap_or_default();
            let mut all_errs = preprocess_errs;
            all_errs.push(BidderError::BadInput(format!(
                "Error in preprocess of Imp, err: {}", first_msg
            )));
            return (vec![], all_errs);
        }

        let body = match serde_json::to_vec(&req) {
            Ok(b) => b,
            Err(_) => return (vec![], vec![BidderError::BadInput("Error in packaging request to JSON".to_string())]),
        };

        let url = build_endpoint(&self.endpoint, request.tmax);

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        if let Some(device) = &request.device {
            if let Some(ua) = &device.ua {
                if !ua.is_empty() { headers.insert("User-Agent".to_string(), ua.clone()); }
            }
            if let Some(ip) = &device.ip {
                if !ip.is_empty() { headers.insert("X-Forwarded-For".to_string(), ip.clone()); }
            }
            if let Some(lang) = &device.language {
                if !lang.is_empty() { headers.insert("Accept-Language".to_string(), lang.clone()); }
            }
            if let Some(dnt) = device.dnt {
                headers.insert("DNT".to_string(), dnt.to_string());
            }
        }
        if let Some(site) = &request.site {
            if let Some(page) = &site.page {
                if !page.is_empty() { headers.insert("Referer".to_string(), page.clone()); }
            }
        }

        (vec![RequestData {
            method: "POST".to_string(),
            uri: url,
            body,
            headers,
            imp_ids: get_imp_ids(&request.imp),
        }], vec![])
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Invalid Status Returned: {}. Run with request.debug = 1 for more info",
                response.status_code
            ))]);
        }

        let bid_resp: BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(
                format!("Unable to unpackage bid response. Error: {}", e)
            )])?;

        let mut result = BidderResponse::with_capacity(1);
        for mut sb in bid_resp.seatbid {
            for bid in sb.bid.iter_mut() {
                // cadent sets bid.impid = bid.id for tracking
                bid.impid = bid.id.clone();
                let bid_type = get_bid_type(bid.adm.as_deref());
                result.bids.push(TypedBid::new(bid.clone(), bid_type));
            }
        }
        Ok(result)
    }
}
