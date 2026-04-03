use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct GammaAdapter {
    pub endpoint: String,
}

impl GammaAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

#[derive(serde::Deserialize)]
struct ExtImpBidder {
    bidder: GammaExt,
}

#[derive(serde::Deserialize)]
struct GammaExt {
    #[serde(rename = "partnerId", default)]
    partner_id: String,
    #[serde(rename = "zoneId", default)]
    zone_id: String,
    #[serde(rename = "webId", default)]
    web_id: String,
}

/// Custom bid response struct to handle Gamma's extended fields.
#[derive(serde::Deserialize)]
struct GammaBidResponse {
    id: String,
    #[serde(default)]
    seatbid: Vec<GammaSeatBid>,
    #[serde(default)]
    cur: Option<String>,
}

#[derive(serde::Deserialize)]
struct GammaSeatBid {
    #[serde(default)]
    bid: Vec<GammaBid>,
}

#[derive(serde::Deserialize)]
struct GammaBid {
    #[serde(flatten)]
    inner: openrtb::Bid,
    #[serde(rename = "vastXml", default)]
    vast_xml: String,
    #[serde(rename = "vastUrl", default)]
    vast_url: String,
}

fn get_media_type_for_imp(imp_id: &str, imps: &[openrtb::Imp]) -> BidType {
    for imp in imps {
        if imp.id == imp_id {
            if imp.video.is_some() {
                return BidType::Video;
            }
            return BidType::Banner;
        }
    }
    BidType::Banner
}

impl Bidder for GammaAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (
                vec![],
                vec![BidderError::BadInput("No impressions in the bid request".to_string())],
            );
        }

        let mut errors = Vec::new();
        let mut valid_imp_indices = Vec::new();

        // Pre-process imps: validate banner/video only, fix banner dims.
        let mut req_copy = request.clone();
        for (i, imp) in req_copy.imp.iter_mut().enumerate() {
            if imp.banner.is_some() {
                if let Some(banner) = &mut imp.banner {
                    if banner.w.is_none() && banner.h.is_none() && !banner.format.is_empty() {
                        let first = banner.format[0].clone();
                        banner.w = Some(first.w);
                        banner.h = Some(first.h);
                    }
                }
                valid_imp_indices.push(i);
            } else if imp.video.is_some() {
                valid_imp_indices.push(i);
            } else {
                errors.push(BidderError::BadInput(format!(
                    "Gamma only supports banner and video media types. Ignoring imp id={}",
                    imp.id
                )));
            }
        }

        if valid_imp_indices.is_empty() {
            errors.push(BidderError::BadInput(
                "No valid impression in the bid request".to_string(),
            ));
            return (vec![], errors);
        }

        let mut requests = Vec::new();

        // Create one GET request per valid impression.
        for idx in valid_imp_indices {
            let imp = &req_copy.imp[idx];

            let gamma_ext = match imp.ext.as_ref()
                .and_then(|e| serde_json::from_str::<ExtImpBidder>(e.get()).ok())
            {
                Some(e) => e.bidder,
                None => {
                    errors.push(BidderError::BadInput("ext.bidder not provided".to_string()));
                    continue;
                }
            };

            if gamma_ext.partner_id.is_empty() {
                errors.push(BidderError::BadInput("PartnerID is empty".to_string()));
                continue;
            }
            if gamma_ext.zone_id.is_empty() {
                errors.push(BidderError::BadInput("ZoneID is empty".to_string()));
                continue;
            }
            if gamma_ext.web_id.is_empty() {
                errors.push(BidderError::BadInput("WebID is empty".to_string()));
                continue;
            }

            let mut uri = format!(
                "{}?id={}&zid={}&wid={}&bidid={}&hb=pbmobile",
                self.endpoint,
                gamma_ext.partner_id,
                gamma_ext.zone_id,
                gamma_ext.web_id,
                imp.id
            );

            if let Some(device) = &request.device {
                if let Some(ip) = &device.ip {
                    if !ip.is_empty() {
                        uri.push_str(&format!("&device_ip={}", ip));
                    }
                }
                if let Some(model) = &device.model {
                    if !model.is_empty() {
                        uri.push_str(&format!("&device_model={}", model));
                    }
                }
                if let Some(os) = &device.os {
                    if !os.is_empty() {
                        uri.push_str(&format!("&device_os={}", os));
                    }
                }
                if let Some(ua) = &device.ua {
                    if !ua.is_empty() {
                        uri.push_str(&format!("&device_ua={}", percent_encode(ua)));
                    }
                }
                if let Some(ifa) = &device.ifa {
                    if !ifa.is_empty() {
                        uri.push_str(&format!("&device_ifa={}", ifa));
                    }
                }
            }

            if let Some(app) = &request.app {
                if let Some(id) = &app.id {
                    if !id.is_empty() {
                        uri.push_str(&format!("&app_id={}", id));
                    }
                }
                if let Some(bundle) = &app.bundle {
                    if !bundle.is_empty() {
                        uri.push_str(&format!("&app_bundle={}", bundle));
                    }
                }
                if let Some(name) = &app.name {
                    if !name.is_empty() {
                        uri.push_str(&format!("&app_name={}", name));
                    }
                }
            }

            let mut headers = HashMap::new();
            headers.insert("Accept".to_string(), "*/*".to_string());
            headers.insert("x-openrtb-version".to_string(), "2.5".to_string());
            headers.insert("Connection".to_string(), "keep-alive".to_string());
            headers.insert("cache-control".to_string(), "no-cache".to_string());
            headers.insert("Accept-Encoding".to_string(), "gzip, deflate".to_string());

            if let Some(device) = &request.device {
                if let Some(ua) = &device.ua {
                    if !ua.is_empty() {
                        headers.insert("User-Agent".to_string(), ua.clone());
                    }
                }
                if let Some(ip) = &device.ip {
                    if !ip.is_empty() {
                        headers.insert("X-Forwarded-For".to_string(), ip.clone());
                    }
                }
                if let Some(lang) = &device.language {
                    if !lang.is_empty() {
                        headers.insert("Accept-Language".to_string(), lang.clone());
                    }
                }
                if let Some(dnt) = device.dnt {
                    headers.insert("DNT".to_string(), dnt.to_string());
                }
            }

            requests.push(RequestData {
                method: "GET".to_string(),
                uri,
                body: vec![],
                headers,
                imp_ids: vec![imp.id.clone()],
            });
        }

        (requests, errors)
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
        if let Err(e) = crate::check_response_status(response.status_code) {
            return Err(vec![e]);
        }

        let gamma_resp: GammaBidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        if gamma_resp.seatbid.is_empty() {
            return Ok(BidderResponse::new());
        }

        let mut result = BidderResponse::with_capacity(5);
        let mut errors = Vec::new();

        for sb in gamma_resp.seatbid {
            for gamma_bid in sb.bid {
                let media_type = get_media_type_for_imp(&gamma_resp.id, &internal.imp);

                let mut bid = gamma_bid.inner;

                if media_type == BidType::Video {
                    if !gamma_bid.vast_xml.is_empty() {
                        if !gamma_bid.vast_url.is_empty() {
                            bid.nurl = Some(gamma_bid.vast_url);
                        }
                        bid.adm = Some(gamma_bid.vast_xml);
                    } else {
                        errors.push(BidderError::BadServerResponse(
                            "Missing Ad Markup. Run with request.debug = 1 for more info".to_string(),
                        ));
                        continue;
                    }
                } else if bid.adm.as_deref().map(|s| s.is_empty()).unwrap_or(true) {
                    errors.push(BidderError::BadServerResponse(
                        "Missing Ad Markup. Run with request.debug = 1 for more info".to_string(),
                    ));
                    continue;
                }

                result.bids.push(TypedBid::new(bid, media_type));
            }
        }

        if errors.is_empty() {
            Ok(result)
        } else {
            // Return partial results alongside errors — mirroring Go behaviour.
            Ok(result)
        }
    }
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9'
            | b'-' | b'_' | b'.' | b'~' => out.push(byte as char),
            b => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}
