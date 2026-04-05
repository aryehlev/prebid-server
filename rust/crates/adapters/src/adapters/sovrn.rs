use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::{Deserialize, Serialize};

pub struct SovrnAdapter {
    pub endpoint: String,
}

impl SovrnAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

#[derive(Serialize, Deserialize, Default)]
struct ExtImpSovrn {
    #[serde(rename = "tagid", default)]
    pub tagid: String,
    #[serde(rename = "TagId", default)]
    pub tag_id: String,
    // bidfloor can be string or number
    #[serde(default)]
    pub bidfloor: serde_json::Value,
}

#[derive(Deserialize, Default)]
struct ImpExt {
    #[serde(default)]
    bidder: ExtImpSovrn,
}

fn get_ext_bid_floor(sovrn_ext: &ExtImpSovrn) -> f64 {
    match &sovrn_ext.bidfloor {
        serde_json::Value::String(s) => s.parse::<f64>().unwrap_or(0.0),
        serde_json::Value::Number(n) => n.as_f64().unwrap_or(0.0),
        _ => 0.0,
    }
}

/// URL-decode percent-encoded string (best-effort).
fn url_decode(s: &str) -> String {
    percent_encoding::percent_decode_str(s)
        .decode_utf8()
        .map(|c| c.into_owned())
        .unwrap_or_else(|_| s.to_string())
}

impl Bidder for SovrnAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut valid_imps = Vec::new();

        for imp in &request.imp {
            // Parse bidder ext
            let imp_ext: ImpExt = match imp.ext.as_ref().and_then(|e| serde_json::from_value(e.clone()).ok()) {
                Some(e) => e,
                None => {
                    errs.push(BidderError::BadInput("Failed to parse sovrn imp ext".to_string()));
                    continue;
                }
            };

            let sovrn_ext = &imp_ext.bidder;

            // Get tagid — prefer lowercase tagid, fall back to TagId
            let tag_id = if !sovrn_ext.tagid.is_empty() {
                sovrn_ext.tagid.clone()
            } else {
                sovrn_ext.tag_id.clone()
            };

            if tag_id.is_empty() {
                errs.push(BidderError::BadInput("Missing required parameter 'tagid'".to_string()));
                continue;
            }

            let mut imp_copy = imp.clone();
            imp_copy.tagid = Some(tag_id);

            // Apply ext bidfloor if imp has no floor set
            let ext_floor = get_ext_bid_floor(sovrn_ext);
            if imp_copy.bidfloor == 0.0 && ext_floor > 0.0 {
                imp_copy.bidfloor = ext_floor;
            }

            // Validate video params if video impression
            if let Some(video) = &imp_copy.video {
                if video.mimes.is_none() || video.maxduration.unwrap_or(0) == 0 || video.protocols.is_none() {
                    errs.push(BidderError::BadInput("Missing required video parameter".to_string()));
                    continue;
                }
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
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let mut headers = HashMap::new();
        // Go uses "application/json" (no charset) for sovrn
        headers.insert("Content-Type".to_string(), "application/json".to_string());

        // Add device headers if present
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
            if let Some(dnt) = &device.dnt {
                headers.insert("DNT".to_string(), dnt.to_string());
            }
        }

        // Add cookie header for buyeruid
        if let Some(user) = &request.user {
            if let Some(buyeruid) = &user.buyeruid {
                let uid = buyeruid.trim().to_string();
                if !uid.is_empty() {
                    headers.insert("Cookie".to_string(), format!("ljt_reader={}", uid));
                }
            }
        }

        let imp_ids = get_imp_ids(&req_copy.imp);

        (
            vec![RequestData {
                method: "POST".to_string(),
                uri: self.endpoint.clone(),
                body,
                headers,
                imp_ids,
            }],
            errs,
        )
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

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(5);
        let mut errs = Vec::new();

        for sb in bid_resp.seatbid {
            for mut bid in sb.bid {
                // URL-decode the AdM field (Go does url.QueryUnescape)
                if let Some(adm) = &bid.adm {
                    bid.adm = Some(url_decode(adm));
                }

                // Find matching imp to determine bid type
                let imp = internal.imp.iter().find(|i| i.id == bid.impid);
                match imp {
                    Some(imp) => {
                        let bid_type = if imp.video.is_some() {
                            BidType::Video
                        } else {
                            BidType::Banner
                        };
                        result.bids.push(TypedBid::new(bid, bid_type));
                    }
                    None => {
                        errs.push(BidderError::BadInput(format!(
                            "Imp ID {} in bid didn't match with any imp in the original request",
                            bid.impid
                        )));
                    }
                }
            }
        }

        if errs.is_empty() {
            Ok(result)
        } else {
            // Non-fatal errors: return partial result
            Ok(result)
        }
    }
}
