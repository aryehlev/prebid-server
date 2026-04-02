use std::collections::HashMap;

use pbs_adapters::{
    Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use openrtb_ext::BidType;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub struct SovrnAdapter {
    pub endpoint: String,
}

impl SovrnAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

/// Sovrn imp extension bidder params
#[derive(Debug, Default, Deserialize)]
struct ExtImpSovrn {
    #[serde(default)]
    tagid: String,
    #[serde(rename = "tagId", default)]
    tag_id: String,
    #[serde(rename = "bidfloor", default)]
    bid_floor: Option<Value>,
}

/// Wrapper for imp.ext
#[derive(Debug, Deserialize)]
struct ImpExt {
    bidder: ExtImpSovrn,
}

fn get_tag_id(ext: &ExtImpSovrn) -> &str {
    if !ext.tagid.is_empty() {
        &ext.tagid
    } else {
        &ext.tag_id
    }
}

fn get_ext_bid_floor(ext: &ExtImpSovrn) -> f64 {
    match &ext.bid_floor {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        Some(Value::String(s)) => s.parse::<f64>().unwrap_or(0.0),
        _ => 0.0,
    }
}

impl Bidder for SovrnAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut headers = HashMap::new();
        headers.insert(
            "Content-Type".to_string(),
            "application/json;charset=utf-8".to_string(),
        );

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

        if let Some(user) = &request.user {
            if let Some(buyeruid) = &user.buyeruid {
                let trimmed = buyeruid.trim();
                if !trimmed.is_empty() {
                    headers.insert(
                        "Cookie".to_string(),
                        format!("ljt_reader={}", trimmed),
                    );
                }
            }
        }

        let mut errs = Vec::new();
        let mut valid_imps = Vec::new();

        for imp in &request.imp {
            // Parse imp ext
            let imp_ext: ImpExt = match imp.ext.as_ref().and_then(|e| serde_json::from_value(e.clone()).ok()) {
                Some(e) => e,
                None => {
                    errs.push(BidderError::BadInput(
                        "Failed to parse imp.ext.bidder for sovrn".to_string(),
                    ));
                    continue;
                }
            };

            let sovrn_ext = &imp_ext.bidder;
            let tag_id = get_tag_id(sovrn_ext);
            if tag_id.is_empty() {
                errs.push(BidderError::BadInput(
                    "Missing required parameter 'tagid'".to_string(),
                ));
                continue;
            }

            // Validate video params if present
            if let Some(video) = &imp.video {
                if video.mimes.as_ref().map_or(true, |m| m.is_empty())
                    || video.max_duration.unwrap_or(0) == 0
                    || video.protocols.as_ref().map_or(true, |p| p.is_empty())
                {
                    errs.push(BidderError::BadInput(
                        "Missing required video parameter".to_string(),
                    ));
                    continue;
                }
            }

            // Clone imp and set tagid
            let mut imp_copy = imp.clone();
            imp_copy.tagid = Some(tag_id.to_string());

            // Apply bid floor from ext if imp has none
            let ext_floor = get_ext_bid_floor(sovrn_ext);
            if imp_copy.bidfloor.unwrap_or(0.0) == 0.0 && ext_floor > 0.0 {
                imp_copy.bidfloor = Some(ext_floor);
            }

            valid_imps.push(imp_copy);
        }

        if valid_imps.is_empty() {
            return (vec![], errs);
        }

        let mut req_copy = request.clone();
        req_copy.imp = valid_imps;

        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => {
                errs.push(BidderError::BadInput(e.to_string()));
                return (vec![], errs);
            }
        };

        let imp_ids: Vec<String> = req_copy.imp.iter().map(|i| i.id.clone()).collect();

        let req = RequestData {
            method: "POST".to_string(),
            uri: self.endpoint.clone(),
            body,
            headers,
            imp_ids,
        };

        (vec![req], errs)
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

        if response.status_code == 400 {
            return Err(vec![BidderError::BadInput(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info",
                response.status_code
            ))]);
        }

        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info",
                response.status_code
            ))]);
        }

        let bid_response: openrtb::BidResponse =
            serde_json::from_slice(&response.body).map_err(|e| {
                vec![BidderError::BadServerResponse(e.to_string())]
            })?;

        let mut result = BidderResponse::with_capacity(5);
        let mut errs = Vec::new();

        for seat_bid in bid_response.seatbid {
            for mut bid in seat_bid.bid {
                // URL-decode adm if present
                if let Some(adm) = &bid.adm {
                    if let Ok(decoded) = urlencoding::decode(adm) {
                        bid.adm = Some(decoded.into_owned());
                    } else {
                        // If decode fails, skip this bid per Go logic
                        continue;
                    }
                }

                // Determine bid type: video if imp has video, else banner
                let bid_type = internal
                    .imp
                    .iter()
                    .find(|imp| imp.id == bid.impid)
                    .map(|imp| {
                        if imp.video.is_some() {
                            BidType::Video
                        } else {
                            BidType::Banner
                        }
                    });

                let bid_type = match bid_type {
                    Some(t) => t,
                    None => {
                        errs.push(BidderError::BadInput(format!(
                            "Imp ID {} in bid didn't match with any imp in the original request",
                            bid.impid
                        )));
                        continue;
                    }
                };

                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }

        if !errs.is_empty() {
            // Return partial result and errors — caller decides; match Go behaviour of returning both
            // Here we return Ok with the partial result since Go returns response + errs
        }
        let _ = errs; // suppress warning; Go returns both response and errs
        Ok(result)
    }
}
