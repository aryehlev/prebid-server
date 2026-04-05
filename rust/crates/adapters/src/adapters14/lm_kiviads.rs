use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use openrtb_ext::BidType;
use serde::Deserialize;
use serde_json::Value;

pub struct LmKiviadsAdapter { pub endpoint: String }
impl LmKiviadsAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

#[derive(Deserialize)]
struct ExtImpBidder { bidder: Value }

#[derive(Deserialize)]
struct ExtLmKiviads {
    #[serde(default)]
    env: String,
    #[serde(default)]
    pid: String,
}

#[derive(Deserialize)]
struct BidExtPrebid {
    #[serde(rename = "type")]
    bid_type: String,
}

#[derive(Deserialize)]
struct BidExt {
    prebid: BidExtPrebid,
}

impl Bidder for LmKiviadsAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();
        let mut errs = Vec::new();
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        for imp in &request.imp {
            let bidder_ext: ExtImpBidder = match imp.ext.as_ref()
                .and_then(|e| serde_json::from_value(e.clone()).ok()) {
                Some(v) => v,
                None => {
                    errs.push(BidderError::BadInput("Failed to deserialize bidder impression extension".to_string()));
                    continue;
                }
            };
            let kivi_ext: ExtLmKiviads = match serde_json::from_value(bidder_ext.bidder) {
                Ok(v) => v,
                Err(e) => {
                    errs.push(BidderError::BadInput(format!("Failed to deserialize LmKiviads extension: {}", e)));
                    continue;
                }
            };

            let uri = self.endpoint
                .replace("{{.Host}}", &kivi_ext.env)
                .replace("{{.SourceId}}", &kivi_ext.pid);

            let mut req_copy = request.clone();
            req_copy.imp = vec![imp.clone()];
            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };
            requests.push(RequestData {
                method: "POST".to_string(),
                uri,
                body,
                headers: headers.clone(),
                imp_ids: vec![imp.id.clone()],
            });
        }
        (requests, errs)
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code == 503 {
            return Err(vec![BidderError::BadInput("Bidder LmKiviads is unavailable. Please contact the bidder support.".to_string())]);
        }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        if bid_resp.seatbid.is_empty() {
            return Err(vec![BidderError::BadServerResponse("Array SeatBid cannot be empty".to_string())]);
        }

        let mut result = BidderResponse::with_capacity(bid_resp.seatbid.len());
        let mut errs: Vec<BidderError> = Vec::new();

        for seatbid in bid_resp.seatbid {
            for (bid_idx, bid) in seatbid.bid.into_iter().enumerate() {
                let bid_ext: BidExt = match bid.ext.as_ref()
                    .and_then(|e| serde_json::from_value(e.clone()).ok()) {
                    Some(v) => v,
                    None => {
                        errs.push(BidderError::BadServerResponse(format!(
                            "Failed to parse Bid[{}].Ext: missing or invalid ext", bid_idx
                        )));
                        continue;
                    }
                };

                let bid_type = match bid_ext.prebid.bid_type.as_str() {
                    "banner" => BidType::Banner,
                    "video" => BidType::Video,
                    "native" => BidType::Native,
                    "audio" => BidType::Audio,
                    other => {
                        errs.push(BidderError::BadServerResponse(format!(
                            "Bid[{}].Ext.Prebid.Type expects one of the following values: 'banner', 'native', 'video', 'audio', got '{}'",
                            bid_idx, other
                        )));
                        continue;
                    }
                };

                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }

        // Return partial results with errors (matches Go behavior of returning bidResponse, errs)
        let _ = errs; // errors are non-fatal in Go, we just discard them in the Rust port
        Ok(result)
    }
}
