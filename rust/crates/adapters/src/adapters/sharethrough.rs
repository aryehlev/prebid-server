use std::collections::HashMap;

use pbs_adapters::{
    Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid,
    get_imp_ids,
};
use openrtb_ext::BidType;
use serde::{Deserialize, Serialize};
use serde_json::Value;

const ADAPTER_VERSION: &str = "10.0";

pub struct SharethroughAdapter {
    pub endpoint: String,
}

impl SharethroughAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

/// Sharethrough imp ext bidder params
#[derive(Debug, Default, Deserialize)]
struct ExtImpSharethrough {
    #[serde(default)]
    pkey: String,
    #[serde(rename = "bcat", default)]
    bcat: Vec<String>,
    #[serde(rename = "badv", default)]
    badv: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ImpExt {
    bidder: ExtImpSharethrough,
}

/// Bid ext for determining bid type
#[derive(Debug, Default, Deserialize)]
struct StrBidExt {
    prebid: Option<StrBidExtPrebid>,
}

#[derive(Debug, Default, Deserialize)]
struct StrBidExtPrebid {
    #[serde(rename = "type", default)]
    bid_type: String,
}

fn parse_bid_type(s: &str) -> Option<BidType> {
    match s {
        "banner" => Some(BidType::Banner),
        "video" => Some(BidType::Video),
        "native" => Some(BidType::Native),
        "audio" => Some(BidType::Audio),
        _ => None,
    }
}

fn get_media_type_for_bid(bid: &openrtb::Bid) -> Result<BidType, BidderError> {
    if let Some(ext) = &bid.ext {
        if let Ok(bid_ext) = serde_json::from_value::<StrBidExt>(ext.clone()) {
            if let Some(prebid) = bid_ext.prebid {
                if !prebid.bid_type.is_empty() {
                    if let Some(t) = parse_bid_type(&prebid.bid_type) {
                        return Ok(t);
                    }
                }
            }
        }
    }

    Err(BidderError::BadServerResponse(format!(
        "Failed to parse bid mediatype for impression \"{}\"",
        bid.impid
    )))
}

/// Split a multi-format impression into separate single-format impressions
fn split_imp_by_media_type(imp: &openrtb::Imp) -> Result<Vec<openrtb::Imp>, BidderError> {
    if imp.banner.is_none() && imp.video.is_none() && imp.native.is_none() {
        return Err(BidderError::BadInput(
            "Invalid MediaType. Sharethrough only supports Banner, Video and Native.".to_string(),
        ));
    }

    let mut result = Vec::new();

    if imp.banner.is_some() {
        let mut imp_copy = imp.clone();
        imp_copy.video = None;
        imp_copy.native = None;
        imp_copy.audio = None;
        result.push(imp_copy);
    }

    if imp.video.is_some() {
        let mut imp_copy = imp.clone();
        imp_copy.banner = None;
        imp_copy.native = None;
        imp_copy.audio = None;
        result.push(imp_copy);
    }

    if imp.native.is_some() {
        let mut imp_copy = imp.clone();
        imp_copy.banner = None;
        imp_copy.video = None;
        imp_copy.audio = None;
        result.push(imp_copy);
    }

    Ok(result)
}

impl Bidder for SharethroughAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut requests = Vec::new();

        let mut headers = HashMap::new();
        headers.insert(
            "Content-Type".to_string(),
            "application/json;charset=utf-8".to_string(),
        );
        headers.insert("Accept".to_string(), "application/json".to_string());

        // Set source ext with str version info
        let mut req_copy = request.clone();
        let mut source = req_copy.source.clone().unwrap_or_default();
        let mut source_ext: serde_json::Map<String, Value> = source.ext.as_ref()
            .and_then(|e| serde_json::from_value(e.clone()).ok())
            .unwrap_or_default();
        source_ext.insert("str".to_string(), Value::String(ADAPTER_VERSION.to_string()));
        source_ext.insert("version".to_string(), Value::String("unknown".to_string()));
        source.ext = Some(Value::Object(source_ext));
        req_copy.source = Some(source);

        for imp in &request.imp {
            let imp_ext: ImpExt = match imp
                .ext
                .as_ref()
                .and_then(|e| serde_json::from_value(e.clone()).ok())
            {
                Some(e) => e,
                None => {
                    errs.push(BidderError::BadInput(
                        "Failed to parse sharethrough imp ext".to_string(),
                    ));
                    continue;
                }
            };

            let str_params = &imp_ext.bidder;

            let mut imp_copy = imp.clone();
            imp_copy.tagid = Some(str_params.pkey.clone());
            imp_copy.audio = None;

            // Merge bcat and badv from imp ext into request
            if !str_params.bcat.is_empty() {
                let existing = req_copy.bcat.get_or_insert_with(Vec::new);
                existing.extend_from_slice(&str_params.bcat);
            }
            if !str_params.badv.is_empty() {
                let existing = req_copy.badv.get_or_insert_with(Vec::new);
                existing.extend_from_slice(&str_params.badv);
            }

            // Split by media type
            let split_imps = match split_imp_by_media_type(&imp_copy) {
                Ok(imps) => imps,
                Err(e) => {
                    errs.push(e);
                    continue;
                }
            };

            for single_imp in split_imps {
                let mut single_req = req_copy.clone();
                single_req.imp = vec![single_imp.clone()];

                let body = match serde_json::to_vec(&single_req) {
                    Ok(b) => b,
                    Err(e) => {
                        errs.push(BidderError::BadInput(e.to_string()));
                        continue;
                    }
                };

                let imp_ids = get_imp_ids(&single_req.imp);
                requests.push(RequestData {
                    method: "POST".to_string(),
                    uri: self.endpoint.clone(),
                    body,
                    headers: headers.clone(),
                    imp_ids,
                });
            }
        }

        (requests, errs)
    }

    fn make_bids(
        &self,
        _internal: &openrtb::BidRequest,
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
                "unexpected status code: {}. Run with request.debug = 1 for more info",
                response.status_code
            ))]);
        }

        let bid_response: openrtb::BidResponse =
            serde_json::from_slice(&response.body)
                .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::new();
        result.currency = "USD".to_string();

        let mut errs = Vec::new();

        for seat_bid in bid_response.seatbid {
            for bid in seat_bid.bid {
                match get_media_type_for_bid(&bid) {
                    Ok(bid_type) => result.bids.push(TypedBid::new(bid, bid_type)),
                    Err(e) => errs.push(e),
                }
            }
        }

        Ok(result)
    }
}
