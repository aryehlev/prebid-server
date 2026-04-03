use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::{Deserialize, Serialize};

pub struct YieldmoAdapter {
    pub endpoint: String,
}

impl YieldmoAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

/// Extension from imp.ext.bidder for Yieldmo
#[derive(Debug, Default, Deserialize)]
struct ExtImpYieldmo {
    #[serde(rename = "placement_id", default)]
    placement_id: String,
}

/// Data field in imp.ext
#[derive(Debug, Default, Deserialize)]
struct ExtData {
    #[serde(rename = "pbadslot", default)]
    pb_adslot: String,
}

/// Full imp ext structure (bidder + data)
#[derive(Debug, Default, Deserialize)]
struct ExtImpBidderYieldmo {
    #[serde(rename = "bidder")]
    bidder: Option<serde_json::Value>,
    #[serde(rename = "data")]
    data: Option<ExtData>,
}

/// The outgoing imp.ext after preprocessing
#[derive(Debug, Default, Serialize)]
struct ImpExt {
    #[serde(rename = "placement_id")]
    placement_id: String,
    #[serde(rename = "gpid", skip_serializing_if = "String::is_empty")]
    gpid: String,
}

/// Bid ext with mediatype
#[derive(Debug, Default, Deserialize)]
struct ExtBid {
    #[serde(rename = "mediatype", default)]
    media_type: String,
}

fn get_media_type_for_imp(bid: &openrtb::Bid) -> Result<BidType, BidderError> {
    let ext_val = bid.ext.as_ref().cloned().unwrap_or(serde_json::Value::Null);
    let ext_bid: ExtBid = serde_json::from_value(ext_val)
        .map_err(|e| BidderError::BadInput(e.to_string()))?;

    match ext_bid.media_type.as_str() {
        "banner" => Ok(BidType::Banner),
        "video" => Ok(BidType::Video),
        "native" => Ok(BidType::Native),
        other => Err(BidderError::BadInput(format!("invalid BidType: {}", other))),
    }
}

/// Preprocess the request: extract placement_id and gpid, rewrite imp.ext
fn preprocess(
    request: &mut openrtb::BidRequest,
    _req_info: &ExtraRequestInfo,
) -> Vec<BidderError> {
    let mut errors = Vec::new();

    for imp in &mut request.imp {
        // Parse the imp ext
        let ext_val = imp.ext.as_ref().cloned().unwrap_or(serde_json::Value::Null);
        let bidder_yieldmo: ExtImpBidderYieldmo = match serde_json::from_value(ext_val) {
            Ok(v) => v,
            Err(e) => {
                errors.push(BidderError::BadInput(e.to_string()));
                continue;
            }
        };

        let bidder_val = bidder_yieldmo.bidder.unwrap_or(serde_json::Value::Null);
        let yieldmo_ext: ExtImpYieldmo = match serde_json::from_value(bidder_val) {
            Ok(v) => v,
            Err(e) => {
                errors.push(BidderError::BadInput(e.to_string()));
                continue;
            }
        };

        let mut imp_ext = ImpExt {
            placement_id: yieldmo_ext.placement_id,
            gpid: String::new(),
        };

        if let Some(data) = bidder_yieldmo.data {
            if !data.pb_adslot.is_empty() {
                imp_ext.gpid = data.pb_adslot;
            }
        }

        imp.ext = match serde_json::to_value(&imp_ext) {
            Ok(v) => Some(v),
            Err(e) => {
                errors.push(BidderError::BadInput(e.to_string()));
                continue;
            }
        };
    }

    errors
}

impl Bidder for YieldmoAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut req_copy = request.clone();
        let mut errors = preprocess(&mut req_copy, req_info);

        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => {
                errors.push(BidderError::BadInput(e.to_string()));
                return (vec![], errors);
            }
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());

        (
            vec![RequestData {
                method: "POST".to_string(),
                uri: self.endpoint.clone(),
                body,
                headers,
                imp_ids: get_imp_ids(&request.imp),
            }],
            errors,
        )
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
                "Unexpected status code: {}. Run with request.debug = 1 for more info",
                response.status_code
            ))]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(1);

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = match get_media_type_for_imp(&bid) {
                    Ok(t) => t,
                    Err(_) => continue, // skip bids with unknown/invalid mediatype
                };
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }

        Ok(result)
    }
}
