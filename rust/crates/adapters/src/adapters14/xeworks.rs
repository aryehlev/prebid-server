use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use openrtb_ext::BidType;
use serde::Deserialize;

pub struct XeworksAdapter {
    pub endpoint: String,
}

impl XeworksAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

/// Extension from imp.ext.bidder for Xeworks
#[derive(Debug, Default, Deserialize)]
struct ExtXeworks {
    #[serde(rename = "env", default)]
    env: String,
    #[serde(rename = "pid", default)]
    pid: String,
}

/// Bid ext structure: {"prebid": {"type": "banner"|"video"|"native"|"audio"}}
#[derive(Debug, Default, Deserialize)]
struct BidExtPrebidType {
    #[serde(rename = "type", default)]
    type_: String,
}

#[derive(Debug, Default, Deserialize)]
struct BidExt {
    #[serde(rename = "prebid", default)]
    prebid: BidExtPrebidType,
}

fn parse_bid_type(type_str: &str) -> Result<BidType, BidderError> {
    match type_str {
        "banner" => Ok(BidType::Banner),
        "video" => Ok(BidType::Video),
        "native" => Ok(BidType::Native),
        "audio" => Ok(BidType::Audio),
        other => Err(BidderError::BadServerResponse(format!(
            "Bid.Ext.Prebid.Type expects one of the following values: 'banner', 'native', 'video', 'audio', got '{}'",
            other
        ))),
    }
}

fn build_endpoint(base_endpoint: &str, imp: &openrtb::Imp) -> Result<String, BidderError> {
    let bidder_val = imp.ext.as_ref()
        .and_then(|e| e.get("bidder"))
        .cloned()
        .ok_or_else(|| BidderError::BadInput(
            "Failed to deserialize bidder impression extension: missing bidder ext".to_string()
        ))?;

    let xeworks_ext: ExtXeworks = serde_json::from_value(bidder_val)
        .map_err(|e| BidderError::BadInput(format!(
            "Failed to deserialize Xeworks extension: {}",
            e
        )))?;

    // The Go code uses a template: endpoint template with {{.Host}} = env and {{.SourceId}} = pid
    // We substitute directly since we have a plain endpoint string
    let endpoint = base_endpoint
        .replace("{{.Host}}", &xeworks_ext.env)
        .replace("{{.SourceId}}", &xeworks_ext.pid);

    Ok(endpoint)
}

impl Bidder for XeworksAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();
        let mut errors = Vec::new();

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        let mut req_copy = request.clone();

        for imp in &request.imp {
            req_copy.imp = vec![imp.clone()];

            let endpoint = match build_endpoint(&self.endpoint, imp) {
                Ok(e) => e,
                Err(e) => {
                    errors.push(e);
                    continue;
                }
            };

            let request_json = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => {
                    errors.push(BidderError::BadInput(e.to_string()));
                    continue;
                }
            };

            requests.push(RequestData {
                method: "POST".to_string(),
                uri: endpoint,
                body: request_json,
                headers: headers.clone(),
                imp_ids: vec![imp.id.clone()],
            });
        }

        (requests, errors)
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
        if response.status_code == 503 {
            return Err(vec![BidderError::BadInput(
                "Bidder Xeworks is unavailable. Please contact the bidder support.".to_string(),
            )]);
        }
        if let Err(e) = crate::check_response_status(response.status_code) {
            return Err(vec![e]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        if bid_resp.seatbid.is_empty() {
            return Err(vec![BidderError::BadServerResponse(
                "Array SeatBid cannot be empty".to_string(),
            )]);
        }

        let mut result = BidderResponse::with_capacity(bid_resp.seatbid.len());
        let mut errors = Vec::new();

        for (_seat_idx, sb) in bid_resp.seatbid.into_iter().enumerate() {
            for (bid_idx, bid) in sb.bid.into_iter().enumerate() {
                let bid_ext_val = bid.ext.as_ref().cloned().unwrap_or(serde_json::Value::Null);
                let bid_ext: BidExt = match serde_json::from_value(bid_ext_val) {
                    Ok(v) => v,
                    Err(e) => {
                        errors.push(BidderError::BadServerResponse(format!(
                            "Failed to parse Bid[{}].Ext: {}",
                            bid_idx, e
                        )));
                        continue;
                    }
                };

                let bid_type = match parse_bid_type(&bid_ext.prebid.type_) {
                    Ok(t) => t,
                    Err(e) => {
                        errors.push(e);
                        continue;
                    }
                };

                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }

        if result.bids.is_empty() && !errors.is_empty() {
            return Err(errors);
        }

        Ok(result)
    }
}
