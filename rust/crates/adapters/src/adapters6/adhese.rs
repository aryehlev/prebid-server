use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde_json::Value;

pub struct AdheseAdapter {
    pub endpoint: String,
}

impl AdheseAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

/// Infer bid type from the first imp's media types
fn infer_bid_type_from_imp(imp: &openrtb::Imp) -> Result<BidType, BidderError> {
    let mut types = Vec::new();
    if imp.banner.is_some() { types.push(BidType::Banner); }
    if imp.video.is_some() { types.push(BidType::Video); }
    if imp.native.is_some() { types.push(BidType::Native); }
    if imp.audio.is_some() { types.push(BidType::Audio); }

    match types.len() {
        1 => Ok(types.remove(0)),
        0 => Err(BidderError::BadServerResponse("Could not infer bid type from imp".to_string())),
        _ => Err(BidderError::BadServerResponse("Multiple media types detected, cannot infer".to_string())),
    }
}

impl Bidder for AdheseAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::BadInput("No impressions in bid request".to_string())]);
        }

        let imp = &request.imp[0];

        // Parse ext.bidder
        let bidder_ext = match imp.ext.as_ref()
            .and_then(|e| e.get("bidder"))
            .cloned()
        {
            Some(v) => v,
            None => return (vec![], vec![BidderError::BadInput("Missing imp.ext.bidder".to_string())]),
        };

        // Extract account, location, format, targets
        let account = bidder_ext.get("account").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let location = bidder_ext.get("location").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let format = bidder_ext.get("format").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let targets_raw = bidder_ext.get("targets").cloned().unwrap_or(Value::Null);

        // Build slot = "{location}-{format}"
        let slot = format!("{}-{}", location, format);

        // Build targets map: start with SL = [slot], then add extra targets
        let mut targets: HashMap<String, Vec<String>> = HashMap::new();
        targets.insert("SL".to_string(), vec![slot]);

        if let Value::Object(extra_targets) = &targets_raw {
            for (k, v) in extra_targets {
                let vals: Vec<String> = match v {
                    Value::Array(arr) => arr.iter()
                        .filter_map(|x| x.as_str().map(|s| s.to_string()))
                        .collect(),
                    Value::String(s) => vec![s.clone()],
                    _ => vec![],
                };
                targets.insert(k.clone(), vals);
            }
        }

        // Build new imp ext: { "adhese": { "SL": [...], ... } }
        let mut adhese_ext: HashMap<String, Value> = HashMap::new();
        let targets_value: HashMap<String, Value> = targets.into_iter()
            .map(|(k, v)| (k, Value::Array(v.into_iter().map(Value::String).collect())))
            .collect();
        adhese_ext.insert("adhese".to_string(), serde_json::to_value(targets_value).unwrap_or(Value::Null));

        let new_imp_ext = match serde_json::to_value(&adhese_ext) {
            Ok(v) => v,
            Err(e) => return (vec![], vec![BidderError::BadInput(format!("Error marshalling modified ext: {}", e))]),
        };

        // Build modified request with new imp ext
        let mut modified_request = request.clone();
        if let Some(imp_mut) = modified_request.imp.first_mut() {
            imp_mut.ext = Some(new_imp_ext);
        }

        // Resolve endpoint: replace {{.AccountID}} with account
        let endpoint = self.endpoint.replace("{{.AccountID}}", &account);

        let body = match serde_json::to_vec(&modified_request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let imp_ids = if let Some(first_id) = request.imp.first().map(|i| i.id.clone()) {
            vec![first_id]
        } else {
            get_imp_ids(&request.imp)
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        (vec![RequestData {
            method: "POST".to_string(),
            uri: endpoint,
            body,
            headers,
            imp_ids,
        }], vec![])
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
        if let Err(e) = crate::check_response_status(response.status_code) {
            return Err(vec![e]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|_| vec![BidderError::BadServerResponse("Empty body".to_string())])?;

        if bid_resp.seatbid.is_empty() {
            return Err(vec![BidderError::BadServerResponse("Empty SeatBid".to_string())]);
        }

        let bids = &bid_resp.seatbid[0].bid;
        if bids.is_empty() {
            return Err(vec![BidderError::BadServerResponse("Empty SeatBid.Bid".to_string())]);
        }

        let mut result = BidderResponse::with_capacity(bids.len());
        if let Some(cur) = &bid_resp.cur {
            if !cur.is_empty() {
                result.currency = cur.clone();
            }
        }

        let first_imp = match internal.imp.first() {
            Some(i) => i,
            None => return Err(vec![BidderError::BadServerResponse("No impressions in request".to_string())]),
        };

        let bid_type = infer_bid_type_from_imp(first_imp)
            .map_err(|e| vec![e])?;

        let mut bid = bids[0].clone();

        // Extract bid.ext["adhese"] and set it as new bid.ext
        if let Some(ext) = &bid.ext {
            if let Value::Object(map) = ext {
                if let Some(adhese_ext) = map.get("adhese") {
                    bid.ext = Some(adhese_ext.clone());
                }
            }
        }

        result.bids.push(TypedBid::new(bid, bid_type));
        Ok(result)
    }
}
