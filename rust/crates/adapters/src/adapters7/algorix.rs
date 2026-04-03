use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;
use serde_json::Value;

pub struct AlgorixAdapter {
    pub endpoint: String,
}

impl AlgorixAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

#[derive(Deserialize)]
struct ExtImpBidder { bidder: Value }

#[derive(Deserialize)]
struct ExtImpAlgorix {
    #[serde(default)]
    sid: String,
    #[serde(default)]
    token: String,
    #[serde(default)]
    region: String,
}

#[derive(Deserialize)]
struct AlgorixBidExt {
    #[serde(rename = "mediaType", default)]
    media_type: String,
}

fn get_region_host(region: &str) -> &str {
    match region {
        "APAC" => "apac.xyz",
        "USE"  => "use.xyz",
        "EUC"  => "euc.xyz",
        _      => "xyz",
    }
}

fn build_endpoint(template: &str, ext: &ExtImpAlgorix) -> String {
    let host = get_region_host(&ext.region);
    template
        .replace("{{.SourceId}}", &urlencoded(&ext.sid))
        .replace("{{.AccountID}}", &urlencoded(&ext.token))
        .replace("{{.Host}}", &urlencoded(host))
}

fn urlencoded(s: &str) -> String {
    // simple percent-encoding of path-unsafe chars (mirrors url::PathEscape)
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9'
            | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => { out.push('%'); out.push_str(&format!("{:02X}", b)); }
        }
    }
    out
}

fn preprocess_request(request: &mut openrtb::BidRequest) {
    for imp in &mut request.imp {
        if let Some(banner) = &mut imp.banner {
            let needs_wh = banner.w.map_or(true, |w| w == 0) || banner.h.map_or(true, |h| h == 0);
            if needs_wh {
                if let Some(formats) = &banner.format {
                    if let Some(first) = formats.first() {
                        let w = first.w;
                        let h = first.h;
                        banner.w = w;
                        banner.h = h;
                    }
                }
            }
        }
    }
}

impl Bidder for AlgorixAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::BadInput("no impressions".to_string())]);
        }

        let ext_val = match &request.imp[0].ext {
            Some(v) => v.clone(),
            None => return (vec![], vec![BidderError::BadInput("Invalid ExtImpAlgoriX value".to_string())]),
        };

        let bidder_ext: ExtImpBidder = match serde_json::from_value(ext_val) {
            Ok(v) => v,
            Err(_) => return (vec![], vec![BidderError::BadInput("Invalid ExtImpAlgoriX value".to_string())]),
        };

        let imp_ext: ExtImpAlgorix = match serde_json::from_value(bidder_ext.bidder) {
            Ok(v) => v,
            Err(_) => return (vec![], vec![BidderError::BadInput("Invalid ExtImpAlgoriX value".to_string())]),
        };

        let url = build_endpoint(&self.endpoint, &imp_ext);

        let mut req = request.clone();
        preprocess_request(&mut req);

        let body = match serde_json::to_vec(&req) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
        headers.insert("x-openrtb-version".to_string(), "2.5".to_string());

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
            return Err(vec![BidderError::BadInput(format!("Unexpected status code: {}.", response.status_code))]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!("Unexpected status code: {}.", response.status_code))]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(5);
        let mut errs = Vec::new();

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = get_bid_type_for_algorix(&bid, &internal.imp, &mut errs);
                match bid_type {
                    Some(t) => result.bids.push(TypedBid::new(bid, t)),
                    None => {} // error already pushed
                }
            }
        }

        if !errs.is_empty() && result.bids.is_empty() {
            return Err(errs);
        }
        Ok(result)
    }
}

fn get_bid_type_for_algorix(
    bid: &openrtb::Bid,
    imps: &[openrtb::Imp],
    errs: &mut Vec<BidderError>,
) -> Option<BidType> {
    // First check bid ext mediaType
    if let Some(ext) = &bid.ext {
        if let Ok(bid_ext) = serde_json::from_value::<AlgorixBidExt>(ext.clone()) {
            match bid_ext.media_type.as_str() {
                "banner" => return Some(BidType::Banner),
                "native" => return Some(BidType::Native),
                "video"  => return Some(BidType::Video),
                _ => {}
            }
        }
    }

    // Fall back to imp type counting
    let mut media_type = BidType::Banner;
    let mut type_cnt = 0;
    for imp in imps {
        if imp.id == bid.impid {
            if imp.banner.is_some() { type_cnt += 1; media_type = BidType::Banner; }
            if imp.native.is_some() { type_cnt += 1; media_type = BidType::Native; }
            if imp.video.is_some()  { type_cnt += 1; media_type = BidType::Video;  }
        }
    }
    if type_cnt == 1 {
        Some(media_type)
    } else {
        errs.push(BidderError::BadServerResponse(format!("unable to fetch mediaType in multi-format: {}", bid.impid)));
        None
    }
}
