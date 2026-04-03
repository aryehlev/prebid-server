use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::{Deserialize, Serialize};

pub struct TappxAdapter { pub endpoint: String }
impl TappxAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

#[derive(Debug, Deserialize)]
struct ExtImpTappx {
    #[serde(rename = "tappxkey", default)]
    tappx_key: String,
    #[serde(rename = "mktag", default)]
    mktag: String,
    #[serde(rename = "bcid", default)]
    bcid: Vec<String>,
    #[serde(rename = "bcrid", default)]
    bcrid: Vec<String>,
    #[serde(rename = "endpoint", default)]
    endpoint: String,
    #[serde(rename = "bidfloor", default)]
    bid_floor: f64,
}

#[derive(Serialize)]
struct TappxBidderExt {
    tappxkey: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    mktag: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    bcid: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    bcrid: Vec<String>,
}

#[derive(Serialize)]
struct TappxExt {
    bidder: TappxBidderExt,
}

fn build_endpoint_url(endpoint_template: &str, tappx_ext: &ExtImpTappx, is_test: bool) -> Result<String, BidderError> {
    if tappx_ext.endpoint.is_empty() {
        return Err(BidderError::BadInput("Tappx endpoint undefined".to_string()));
    }
    if tappx_ext.tappx_key.is_empty() {
        return Err(BidderError::BadInput("Tappx key undefined".to_string()));
    }

    let ep = &tappx_ext.endpoint;
    let is_new_endpoint = {
        let re = regex_lite(&ep);
        re
    };

    let tappx_host = if is_new_endpoint {
        format!("{}.pub.tappx.com/rtb/", ep)
    } else {
        "ssp.api.tappx.com/rtb/v2/".to_string()
    };

    let host = endpoint_template.replace("{{.Host}}", &tappx_host);

    let mut uri = if is_new_endpoint {
        host
    } else {
        format!("{}{}", host, ep)
    };

    let mut params = vec![
        format!("tappxkey={}", url_encode(&tappx_ext.tappx_key)),
        "v=1.6".to_string(),
        "type_cnn=prebid".to_string(),
    ];

    if !is_test {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        params.push(format!("ts={}", ts));
    }

    let query = params.join("&");
    if uri.contains('?') {
        uri = format!("{}&{}", uri, query);
    } else {
        uri = format!("{}?{}", uri, query);
    }

    Ok(uri)
}

fn regex_lite(ep: &str) -> bool {
    // Match pattern: (zz|vz)[0-9]{3,}([a-z]{2,3}|test)
    if !ep.starts_with("zz") && !ep.starts_with("vz") {
        return false;
    }
    let rest = &ep[2..];
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.len() < 3 {
        return false;
    }
    let suffix = &rest[digits.len()..];
    if suffix == "test" {
        return true;
    }
    suffix.len() >= 2 && suffix.len() <= 3 && suffix.chars().all(|c| c.is_ascii_lowercase())
}

fn url_encode(s: &str) -> String {
    s.chars().map(|c| {
        if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '~' {
            c.to_string()
        } else {
            format!("%{:02X}", c as u32)
        }
    }).collect()
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

impl Bidder for TappxAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::BadInput("No impression in the bid request".to_string())]);
        }

        let bidder_val = match request.imp[0].ext.as_ref()
            .and_then(|e| e.get("bidder"))
        {
            Some(v) => v.clone(),
            None => return (vec![], vec![BidderError::BadInput("Error parsing bidderExt object".to_string())]),
        };

        let tappx_ext: ExtImpTappx = match serde_json::from_value(bidder_val) {
            Ok(e) => e,
            Err(_) => return (vec![], vec![BidderError::BadInput("Error parsing tappxExt parameters".to_string())]),
        };

        let is_test = request.test.unwrap_or(0) != 0;
        let url = match build_endpoint_url(&self.endpoint, &tappx_ext, is_test) {
            Ok(u) => u,
            Err(e) => return (vec![], vec![e]),
        };

        let mut req_copy = request.clone();

        // Set bid floor from ext if provided
        if tappx_ext.bid_floor > 0.0 {
            req_copy.imp[0].bidfloor = Some(tappx_ext.bid_floor);
        }

        // Set request ext with tappx bidder info
        let ext_obj = TappxExt {
            bidder: TappxBidderExt {
                tappxkey: tappx_ext.tappx_key.clone(),
                mktag: tappx_ext.mktag.clone(),
                bcid: tappx_ext.bcid.clone(),
                bcrid: tappx_ext.bcrid.clone(),
            },
        };
        req_copy.ext = serde_json::to_value(&ext_obj).ok();

        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(_) => return (vec![], vec![BidderError::BadInput("Error parsing reqJSON object".to_string())]),
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        (vec![RequestData {
            method: "POST".to_string(),
            uri: url,
            body,
            headers,
            imp_ids: get_imp_ids(&request.imp),
        }], vec![])
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
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
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(5);
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = get_media_type_for_imp(&bid.impid, &internal.imp);
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        Ok(result)
    }
}
