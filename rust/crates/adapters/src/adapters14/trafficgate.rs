use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use openrtb_ext::BidType;
use serde::Deserialize;

pub struct TrafficgateAdapter { pub endpoint: String }
impl TrafficgateAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

#[derive(Debug, Deserialize, PartialEq, Eq, Hash, Clone)]
struct ExtImpTrafficGate {
    #[serde(rename = "host", default)]
    host: String,
    #[serde(rename = "sourceId", default)]
    source_id: String,
}

fn get_media_type_for_imp(bid_type: &str) -> BidType {
    match bid_type {
        "video" => BidType::Video,
        "native" => BidType::Native,
        "audio" => BidType::Audio,
        _ => BidType::Banner,
    }
}

impl Bidder for TrafficgateAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let headers: HashMap<String, String> = {
            let mut h = HashMap::new();
            h.insert("Content-Type".to_string(), "application/json".to_string());
            h.insert("Accept".to_string(), "application/json".to_string());
            h
        };

        // Split impressions by host
        let mut imp_groups: HashMap<String, (ExtImpTrafficGate, Vec<openrtb::Imp>)> = HashMap::new();
        let mut errs = Vec::new();

        for imp in &request.imp {
            let bidder_val = match imp.ext.as_ref().and_then(|e| e.get("bidder")) {
                Some(v) => v.clone(),
                None => {
                    errs.push(BidderError::BadInput("Missing bidder ext".to_string()));
                    return (vec![], errs);
                }
            };
            let ext: ExtImpTrafficGate = match serde_json::from_value(bidder_val) {
                Ok(e) => e,
                Err(_) => {
                    errs.push(BidderError::BadInput("Bidder parameters required".to_string()));
                    return (vec![], errs);
                }
            };

            let key = ext.host.clone();
            let entry = imp_groups.entry(key).or_insert_with(|| (ext, Vec::new()));
            entry.1.push(imp.clone());
        }

        let mut requests = Vec::new();
        for (_, (ext, imps)) in imp_groups {
            let url = self.endpoint.replace("{{.Host}}", &ext.host);
            let mut req_copy = request.clone();
            req_copy.imp = imps;

            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };

            let imp_ids = req_copy.imp.iter().map(|i| i.id.clone()).collect();
            requests.push(RequestData {
                method: "POST".to_string(),
                uri: url,
                body,
                headers: headers.clone(),
                imp_ids,
            });
        }

        (requests, errs)
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!("Error response with status {}", response.status_code))]);
        }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(internal.imp.len());
        if let Some(cur) = &bid_resp.cur {
            result.currency = cur.clone();
        }
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type_str = bid.ext.as_ref()
                    .and_then(|e| e.get("prebid"))
                    .and_then(|p| p.get("type"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if bid_type_str.is_empty() {
                    return Err(vec![BidderError::BadServerResponse("Unable to read bid.ext.prebid.type".to_string())]);
                }
                result.bids.push(TypedBid::new(bid, get_media_type_for_imp(&bid_type_str)));
            }
        }
        Ok(result)
    }
}
