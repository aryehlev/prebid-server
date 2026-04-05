use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;
use serde_json::Value;

pub struct BematterfullAdapter {
    pub endpoint: String,
}

impl BematterfullAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

#[derive(Deserialize)]
struct ExtImpBidder {
    bidder: Value,
}

#[derive(Deserialize)]
struct ExtBematterfull {
    #[serde(rename = "env", default)]
    env: String,
    #[serde(rename = "pid", default)]
    pid: String,
}

#[derive(Deserialize, Default)]
struct BidExtPrebid {
    #[serde(rename = "type", default)]
    bid_type: String,
}

#[derive(Deserialize, Default)]
struct BidExt {
    #[serde(default)]
    prebid: BidExtPrebid,
}

fn parse_bid_type(s: &str) -> Result<BidType, BidderError> {
    match s {
        "banner" => Ok(BidType::Banner),
        "video" => Ok(BidType::Video),
        "native" => Ok(BidType::Native),
        "audio" => Ok(BidType::Audio),
        other => Err(BidderError::BadServerResponse(format!(
            "Bid.Ext.Prebid.Type expects one of the following values: 'banner', 'native', 'video', 'audio', got '{}'", other
        ))),
    }
}

impl Bidder for BematterfullAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        let mut results = Vec::new();
        let mut errs = Vec::new();

        for imp in request.imp.iter() {
            let ext_val = match &imp.ext {
                Some(v) => v.clone(),
                None => {
                    errs.push(BidderError::BadInput(format!(
                        "Failed to deserialize bidder impression extension: missing ext"
                    )));
                    continue;
                }
            };

            let imp_ext: ExtImpBidder = match serde_json::from_value(ext_val) {
                Ok(v) => v,
                Err(e) => {
                    errs.push(BidderError::BadInput(format!(
                        "Failed to deserialize bidder impression extension: {}", e
                    )));
                    continue;
                }
            };

            let bm_ext: ExtBematterfull = match serde_json::from_value(imp_ext.bidder) {
                Ok(v) => v,
                Err(e) => {
                    errs.push(BidderError::BadInput(format!(
                        "Failed to deserialize Bematterfull extension: {}", e
                    )));
                    continue;
                }
            };

            // Build URL: replace {{.Host}} with env, {{.SourceId}} with pid
            let url = self.endpoint
                .replace("{{.Host}}", &bm_ext.env)
                .replace("{{.SourceId}}", &bm_ext.pid);

            let mut req_copy = request.clone();
            req_copy.imp = vec![imp.clone()];

            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(BidderError::BadInput(e.to_string()));
                    continue;
                }
            };

            results.push(RequestData {
                method: "POST".to_string(),
                uri: url,
                body,
                headers: headers.clone(),
                imp_ids: get_imp_ids(&req_copy.imp),
            });
        }

        (results, errs)
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code == 503 {
            return Err(vec![BidderError::BadInput(
                "Bidder Bematterfull is unavailable. Please contact the bidder support.".to_string()
            )]);
        }
        if let Err(e) = crate::check_response_status(response.status_code) {
            return Err(vec![e]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        if bid_resp.seatbid.is_empty() {
            return Err(vec![BidderError::BadServerResponse("Array SeatBid cannot be empty".to_string())]);
        }

        let mut result = BidderResponse::with_capacity(bid_resp.seatbid.len());
        let mut errs = Vec::new();

        for sb in &bid_resp.seatbid {
            for (bid_idx, bid) in sb.bid.iter().enumerate() {
                let bid_ext: BidExt = if let Some(ext) = &bid.ext {
                    match serde_json::from_value(ext.clone()) {
                        Ok(e) => e,
                        Err(e) => {
                            errs.push(BidderError::BadServerResponse(format!(
                                "Failed to parse Bid[{}].Ext: {}", bid_idx, e
                            )));
                            continue;
                        }
                    }
                } else {
                    errs.push(BidderError::BadServerResponse(format!(
                        "Failed to parse Bid[{}].Ext: missing ext", bid_idx
                    )));
                    continue;
                };

                match parse_bid_type(&bid_ext.prebid.bid_type) {
                    Ok(bt) => result.bids.push(TypedBid::new(bid.clone(), bt)),
                    Err(e) => errs.push(e),
                }
            }
        }

        Ok(result)
    }
}
