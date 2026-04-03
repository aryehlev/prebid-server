use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct ImprovedigitalAdapter {
    pub endpoint: String,
}

impl ImprovedigitalAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

const PUBLISHER_ENDPOINT_PARAM: &str = "{PublisherId}";

#[derive(serde::Deserialize, Default)]
struct ImpExtBidder {
    bidder: ImprovedigitalBidderParams,
}

#[derive(serde::Deserialize, Default)]
struct ImprovedigitalBidderParams {
    #[serde(rename = "publisherId", default)]
    publisher_id: i64,
}

/// Bid extension for line item and buying type.
#[derive(serde::Deserialize, Default)]
struct BidExt {
    #[serde(default)]
    improvedigital: BidExtImprovedigital,
}

#[derive(serde::Deserialize, Default)]
struct BidExtImprovedigital {
    #[serde(rename = "line_item_id", default)]
    line_item_id: i64,
    #[serde(rename = "buying_type", default)]
    buying_type: String,
}

fn is_multi_format(imp: &openrtb::Imp) -> bool {
    let mut count = 0;
    if imp.banner.is_some() { count += 1; }
    if imp.video.is_some() { count += 1; }
    if imp.audio.is_some() { count += 1; }
    if imp.native.is_some() { count += 1; }
    count > 1
}

fn get_bid_type(bid: &openrtb::Bid, imp: &openrtb::Imp) -> Result<BidType, BidderError> {
    let mtype = bid.mtype.unwrap_or(0);

    let resolved_mtype = if mtype == 0 {
        // Try to infer from imp.
        if is_multi_format(imp) {
            return Err(BidderError::BadServerResponse(format!(
                "Bid must have non-zero MType for multi format impression with ID: \"{}\"",
                bid.impid
            )));
        }
        if imp.banner.is_some() { 1 }
        else if imp.video.is_some() { 2 }
        else if imp.audio.is_some() { 3 }
        else if imp.native.is_some() { 4 }
        else {
            return Err(BidderError::BadServerResponse(format!(
                "Could not determine MType from impression with ID: \"{}\"",
                bid.impid
            )));
        }
    } else {
        mtype
    };

    match resolved_mtype {
        1 => Ok(BidType::Banner),
        2 => Ok(BidType::Video),
        3 => Ok(BidType::Audio),
        4 => Ok(BidType::Native),
        _ => Err(BidderError::BadServerResponse(format!(
            "Unsupported MType {} for impression with ID: \"{}\"",
            resolved_mtype, bid.impid
        ))),
    }
}

fn build_endpoint_url(endpoint: &str, imp: &openrtb::Imp) -> String {
    let publisher_segment = imp.ext.as_ref()
        .and_then(|e| serde_json::from_str::<ImpExtBidder>(e.get()).ok())
        .filter(|e| e.bidder.publisher_id != 0)
        .map(|e| format!("{}/", e.bidder.publisher_id))
        .unwrap_or_default();

    endpoint.replace(PUBLISHER_ENDPOINT_PARAM, &publisher_segment)
}

fn get_imp_ext_with_rewarded(imp: &openrtb::Imp) -> Option<serde_json::Value> {
    let ext_val = imp.ext.as_ref()?;
    let map: HashMap<String, serde_json::Value> = serde_json::from_str(ext_val.get()).ok()?;

    let prebid_val = map.get("prebid")?;
    let prebid_map: HashMap<String, serde_json::Value> = serde_json::from_value(prebid_val.clone()).ok()?;

    let rewarded = prebid_map.get("is_rewarded_inventory")?;
    if rewarded.as_str() == Some("1") {
        let mut new_map = map.clone();
        new_map.insert("is_rewarded_inventory".to_string(), serde_json::Value::Bool(true));
        Some(serde_json::Value::Object(new_map.into_iter().collect()))
    } else {
        None
    }
}

fn deal_regex_matches(buying_type: &str) -> bool {
    buying_type.contains("classic") || buying_type.contains("deal")
}

impl Bidder for ImprovedigitalAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();
        let mut errors = Vec::new();

        // One request per impression.
        for imp in &request.imp {
            let mut imp_copy = imp.clone();

            // Handle rewarded inventory ext promotion.
            if let Some(new_ext) = get_imp_ext_with_rewarded(imp) {
                imp_copy.ext = Some(new_ext);
            }

            let uri = build_endpoint_url(&self.endpoint, imp);

            let mut req_copy = request.clone();
            req_copy.imp = vec![imp_copy];

            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => {
                    errors.push(BidderError::BadInput(e.to_string()));
                    continue;
                }
            };

            let mut headers = HashMap::new();
            headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());

            requests.push(RequestData {
                method: "POST".to_string(),
                uri,
                body,
                headers,
                imp_ids: get_imp_ids(&req_copy.imp),
            });
        }

        (requests, errors)
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

        if bid_resp.seatbid.is_empty() {
            return Ok(BidderResponse::new());
        }

        if bid_resp.seatbid.len() > 1 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected SeatBid! Must be only one but have: {}",
                bid_resp.seatbid.len()
            ))]);
        }

        let seat_bid = &bid_resp.seatbid[0];
        if seat_bid.bid.is_empty() {
            return Ok(BidderResponse::new());
        }

        // Build imp map for fast lookup.
        let imp_map: HashMap<&str, &openrtb::Imp> = internal
            .imp
            .iter()
            .map(|i| (i.id.as_str(), i))
            .collect();

        let mut result = BidderResponse::with_capacity(seat_bid.bid.len());
        if let Some(cur) = &bid_resp.cur {
            result.currency = cur.clone();
        }

        for bid in &seat_bid.bid {
            let imp = match imp_map.get(bid.impid.as_str()) {
                Some(i) => *i,
                None => {
                    return Err(vec![BidderError::BadServerResponse(format!(
                        "Failed to find impression for ID: \"{}\"",
                        bid.impid
                    ))]);
                }
            };

            let bid_type = match get_bid_type(bid, imp) {
                Ok(t) => t,
                Err(e) => return Err(vec![e]),
            };

            let mut bid_copy = bid.clone();

            // Set deal ID from line item if buying type matches deal pattern.
            if let Some(ext_val) = &bid.ext {
                if let Ok(bid_ext) = serde_json::from_str::<BidExt>(ext_val.get()) {
                    let id_ext = &bid_ext.improvedigital;
                    if id_ext.line_item_id != 0 && deal_regex_matches(&id_ext.buying_type) {
                        bid_copy.dealid = Some(id_ext.line_item_id.to_string());
                    }
                }
            }

            result.bids.push(TypedBid::new(bid_copy, bid_type));
        }

        Ok(result)
    }
}
