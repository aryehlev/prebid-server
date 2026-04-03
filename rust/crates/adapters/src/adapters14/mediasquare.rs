use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use openrtb_ext::BidType;
use serde::{Deserialize, Serialize};

pub struct MediasquareAdapter { pub endpoint: String }
impl MediasquareAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

#[derive(Debug, Deserialize, Clone)]
struct MsqExt {
    #[serde(default)]
    owner: String,
    #[serde(default)]
    code: String,
}

#[derive(Debug, Serialize, Clone)]
struct MsqParameters {
    codes: Vec<MsqParametersCodes>,
    gdpr: MsqParametersGdpr,
    #[serde(rename = "type")]
    req_type: String,
    #[serde(rename = "pbjs", skip_serializing_if = "String::is_empty")]
    prebid_ver: String,
    tech: MsqSupport,
    test: bool,
    #[serde(rename = "user_uid", skip_serializing_if = "String::is_empty")]
    user_uid: String,
}

#[derive(Debug, Serialize, Clone)]
struct MsqParametersGdpr {
    consent_required: bool,
    consent_string: String,
}

#[derive(Debug, Serialize, Clone)]
struct MsqParametersCodes {
    adunit: String,
    auctionid: String,
    bidid: String,
    code: String,
    owner: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    mediatypes: Option<MsqMediaTypes>,
}

#[derive(Debug, Serialize, Clone)]
struct MsqMediaTypes {
    #[serde(skip_serializing_if = "Option::is_none")]
    banner: Option<MsqMediaTypeBanner>,
    #[serde(skip_serializing_if = "Option::is_none")]
    video: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    native_request: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
struct MsqMediaTypeBanner {
    sizes: Vec<Vec<Option<i32>>>,
}

#[derive(Debug, Serialize, Clone)]
struct MsqSupport {
    #[serde(skip_serializing_if = "Option::is_none")]
    device: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    app: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    site: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    user: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Clone)]
struct MsqResponse {
    responses: Option<Vec<MsqResponseBid>>,
}

#[derive(Debug, Deserialize, Clone)]
struct MsqResponseBid {
    #[serde(default)]
    id: String,
    #[serde(default)]
    ad: String,
    #[serde(rename = "bid_id", default)]
    bid_id: String,
    #[serde(default)]
    cpm: f64,
    #[serde(default)]
    currency: String,
    #[serde(rename = "creative_id", default)]
    creative_id: String,
    #[serde(default)]
    height: i64,
    #[serde(default)]
    width: i64,
    #[serde(default)]
    adomain: Option<Vec<String>>,
    #[serde(default)]
    burl: String,
    video: Option<serde_json::Value>,
    native: Option<serde_json::Value>,
}

fn get_gdpr_consent_required(request: &openrtb::BidRequest) -> bool {
    request.regs.as_ref()
        .and_then(|r| r.gdpr)
        .map(|v| v == 1)
        .unwrap_or(false)
}

fn get_gdpr_consent_string(request: &openrtb::BidRequest) -> String {
    // First check user.consent
    if let Some(user) = &request.user {
        if let Some(consent) = &user.consent {
            if !consent.is_empty() {
                return consent.clone();
            }
        }
        // Fallback to user.ext.consent
        if let Some(ext) = &user.ext {
            if let Some(c) = ext.get("consent").and_then(|v| v.as_str()) {
                return c.to_string();
            }
        }
    }
    String::new()
}

fn get_user_uid(request: &openrtb::BidRequest) -> String {
    request.user.as_ref()
        .and_then(|u| u.buyeruid.as_deref())
        .unwrap_or("")
        .to_string()
}

fn to_json_value(v: &openrtb::BidRequest) -> Option<serde_json::Value> {
    None // used in tech fields
}

fn build_msq_media_types(imp: &openrtb::Imp) -> Option<MsqMediaTypes> {
    let mut mediatypes = MsqMediaTypes {
        banner: None,
        video: None,
        native_request: None,
    };
    let mut has_content = false;

    if let Some(banner) = &imp.banner {
        let mut sizes: Vec<Vec<Option<i32>>> = Vec::new();
        if let Some(formats) = &banner.format {
            for f in formats {
                sizes.push(vec![f.w, f.h]);
            }
        } else if banner.w.is_some() && banner.h.is_some() {
            sizes.push(vec![banner.w, banner.h]);
        }
        if !sizes.is_empty() {
            mediatypes.banner = Some(MsqMediaTypeBanner { sizes });
            has_content = true;
        }
    }

    if let Some(video) = &imp.video {
        if let Ok(v) = serde_json::to_value(video) {
            mediatypes.video = Some(v);
            has_content = true;
        }
    }

    if let Some(native) = &imp.native {
        let request_str = native.request.as_deref().unwrap_or("");
        if !request_str.is_empty() {
            mediatypes.native_request = Some(request_str.to_string());
            has_content = true;
        }
    }

    if has_content { Some(mediatypes) } else { None }
}

impl Bidder for MediasquareAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::BadInput("<MakeRequests> request: is empty.".to_string())]);
        }

        let mut errs = Vec::new();
        let mut codes: Vec<MsqParametersCodes> = Vec::new();

        for imp in &request.imp {
            let bidder_val = match imp.ext.as_ref().and_then(|e| e.get("bidder")) {
                Some(v) => v.clone(),
                None => {
                    errs.push(BidderError::BadInput("<MakeRequests> imp[ext]: is empty.".to_string()));
                    continue;
                }
            };
            let msq_ext: MsqExt = match serde_json::from_value(bidder_val) {
                Ok(e) => e,
                Err(e) => {
                    errs.push(BidderError::BadInput(format!("<MakeRequests> imp-bidder[ext]: {}", e)));
                    continue;
                }
            };

            let mediatypes = build_msq_media_types(imp);
            if mediatypes.is_none() {
                continue; // skip imps with no supported media types
            }

            codes.push(MsqParametersCodes {
                adunit: imp.tagid.as_deref().unwrap_or("").to_string(),
                auctionid: request.id.clone(),
                bidid: imp.id.clone(),
                code: msq_ext.code,
                owner: msq_ext.owner,
                mediatypes,
            });
        }

        // Build tech support info from request
        let tech = MsqSupport {
            device: request.device.as_ref().and_then(|d| serde_json::to_value(d).ok()),
            app: request.app.as_ref().and_then(|a| serde_json::to_value(a).ok()),
            site: request.site.as_ref().and_then(|s| serde_json::to_value(s).ok()),
            user: request.user.as_ref().and_then(|u| serde_json::to_value(u).ok()),
        };

        let msq_params = MsqParameters {
            codes,
            gdpr: MsqParametersGdpr {
                consent_required: get_gdpr_consent_required(request),
                consent_string: get_gdpr_consent_string(request),
            },
            req_type: "pbs".to_string(),
            prebid_ver: "n/a".to_string(),
            tech,
            test: request.test == Some(1),
            user_uid: get_user_uid(request),
        };

        let body = match serde_json::to_vec(&msq_params) {
            Ok(b) => b,
            Err(e) => {
                errs.push(BidderError::BadInput(format!("<makeRequest> jsonutil.Marshal: {}", e)));
                return (vec![], errs);
            }
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        let imp_ids: Vec<String> = request.imp.iter().map(|i| i.id.clone()).collect();

        (vec![RequestData {
            method: "POST".to_string(),
            uri: self.endpoint.clone(),
            body,
            headers,
            imp_ids,
        }], errs)
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        match response.status_code {
            200 => {},
            400 => return Err(vec![BidderError::BadInput(format!(
                "<MakeBids> Unexpected status code: {}.", response.status_code
            ))]),
            _ => return Err(vec![BidderError::BadServerResponse(format!(
                "<MakeBids> Unexpected status code: {}. Run with request.debug = 1 for more info.",
                response.status_code
            ))]),
        }

        let msq_resp: MsqResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(format!(
                "<MakeBids> Unexpected status code: 406. Bad server response: {}.", e
            ))])?;

        let responses = match msq_resp.responses {
            Some(r) if !r.is_empty() => r,
            _ => return Err(vec![BidderError::BadServerResponse(
                "<MakeBids> Unexpected status code: 204. No responses found into body content.".to_string()
            )]),
        };

        let mut result = BidderResponse::with_capacity(internal.imp.len());

        for resp in &responses {
            let bid_type = if resp.video.is_some() {
                BidType::Video
            } else if resp.native.is_some() {
                BidType::Native
            } else {
                BidType::Banner
            };

            let mtype: Option<i32> = if resp.video.is_some() {
                Some(2) // MarkupVideo
            } else if resp.native.is_some() {
                Some(4) // MarkupNative
            } else {
                Some(1) // MarkupBanner
            };

            let bid = openrtb::Bid {
                id: resp.id.clone(),
                impid: resp.bid_id.clone(),
                price: resp.cpm,
                adm: if resp.ad.is_empty() { None } else { Some(resp.ad.clone()) },
                adomain: resp.adomain.clone(),
                w: Some(resp.width as i32),
                h: Some(resp.height as i32),
                crid: if resp.creative_id.is_empty() { None } else { Some(resp.creative_id.clone()) },
                mtype,
                burl: if resp.burl.is_empty() { None } else { Some(resp.burl.clone()) },
                ..Default::default()
            };

            result.currency = resp.currency.clone();
            result.bids.push(TypedBid::new(bid, bid_type));
        }

        Ok(result)
    }
}
