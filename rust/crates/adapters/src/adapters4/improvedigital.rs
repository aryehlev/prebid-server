use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use openrtb_ext::BidType;
use serde::Deserialize;

pub struct ImprovedigitalAdapter { pub endpoint: String }
impl ImprovedigitalAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

const PUBLISHER_ENDPOINT_PARAM: &str = "{PublisherId}";
const IS_REWARDED_INVENTORY: &str = "is_rewarded_inventory";

#[derive(Debug, Deserialize, Default)]
struct ImpExtBidder {
    bidder: Option<ImpExtBidderData>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ImpExtBidderData {
    #[serde(default)]
    publisher_id: i64,
}

#[derive(Debug, Deserialize, Default)]
struct BidExtImprovedigital {
    #[serde(default)]
    line_item_id: i64,
    #[serde(default)]
    buying_type: String,
}

#[derive(Debug, Deserialize, Default)]
struct BidExt {
    improvedigital: Option<BidExtImprovedigital>,
}

fn is_deal_buying_type(buying_type: &str) -> bool {
    buying_type.contains("classic") || buying_type.contains("deal")
}

fn is_multi_format_imp(imp: &openrtb::Imp) -> bool {
    let mut count = 0;
    if imp.banner.is_some() { count += 1; }
    if imp.video.is_some() { count += 1; }
    if imp.audio.is_some() { count += 1; }
    if imp.native.is_some() { count += 1; }
    count > 1
}

fn get_bid_type(bid: &mut openrtb::Bid, imp: Option<&openrtb::Imp>) -> Result<BidType, BidderError> {
    let imp = match imp {
        Some(i) => i,
        None => return Err(BidderError::BadServerResponse(format!(
            "Failed to find impression for ID: \"{}\"", bid.impid
        ))),
    };

    if bid.mtype.is_none() || bid.mtype == Some(0) {
        if is_multi_format_imp(imp) {
            return Err(BidderError::BadServerResponse(format!(
                "Bid must have non-zero MType for multi format impression with ID: \"{}\"", bid.impid
            )));
        }
        // Determine MType from impression
        if imp.banner.is_some() {
            bid.mtype = Some(1);
        } else if imp.video.is_some() {
            bid.mtype = Some(2);
        } else if imp.audio.is_some() {
            bid.mtype = Some(3);
        } else if imp.native.is_some() {
            bid.mtype = Some(4);
        } else {
            return Err(BidderError::BadServerResponse(format!(
                "Could not determine MType from impression with ID: \"{}\"", bid.impid
            )));
        }
    }

    match bid.mtype.unwrap_or(0) {
        1 => Ok(BidType::Banner),
        2 => Ok(BidType::Video),
        3 => Ok(BidType::Audio),
        4 => Ok(BidType::Native),
        m => Err(BidderError::BadServerResponse(format!(
            "Unsupported MType {} for impression with ID: \"{}\"", m, bid.impid
        ))),
    }
}

fn get_rewarded_imp_ext(imp: &openrtb::Imp) -> Option<serde_json::Value> {
    let ext = imp.ext.as_ref()?;
    let prebid = ext.get("prebid")?;
    let rewarded = prebid.get(IS_REWARDED_INVENTORY)?;
    if rewarded.as_str() == Some("1") || rewarded.as_i64() == Some(1) {
        let mut new_ext = ext.clone();
        if let serde_json::Value::Object(ref mut map) = new_ext {
            map.insert(IS_REWARDED_INVENTORY.to_string(), serde_json::Value::Bool(true));
        }
        Some(new_ext)
    } else {
        None
    }
}

fn build_endpoint_url(endpoint: &str, imp: &openrtb::Imp) -> String {
    let publisher_segment = if let Some(ext) = &imp.ext {
        let bidder_ext: Result<ImpExtBidder, _> = serde_json::from_value(ext.clone());
        if let Ok(b) = bidder_ext {
            if let Some(data) = b.bidder {
                if data.publisher_id != 0 {
                    format!("{}/", data.publisher_id)
                } else {
                    String::new()
                }
            } else {
                String::new()
            }
        } else {
            String::new()
        }
    } else {
        String::new()
    };
    endpoint.replace(PUBLISHER_ENDPOINT_PARAM, &publisher_segment)
}

impl Bidder for ImprovedigitalAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();
        let mut errs = Vec::new();

        for imp in &request.imp {
            let mut imp_copy = imp.clone();

            // Handle rewarded inventory
            if let Some(new_ext) = get_rewarded_imp_ext(imp) {
                imp_copy.ext = Some(new_ext);
            }

            let mut req_copy = request.clone();
            req_copy.imp = vec![imp_copy];

            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };

            let uri = build_endpoint_url(&self.endpoint, imp);

            let mut headers = HashMap::new();
            headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());

            requests.push(RequestData {
                method: "POST".to_string(),
                uri,
                body,
                headers,
                imp_ids: vec![imp.id.clone()],
            });
        }

        (requests, errs)
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

        if bid_resp.seatbid.is_empty() { return Ok(BidderResponse::new()); }
        if bid_resp.seatbid.len() > 1 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected SeatBid! Must be only one but have: {}", bid_resp.seatbid.len()
            ))]);
        }

        let seat_bid = &bid_resp.seatbid[0];
        if seat_bid.bid.is_empty() { return Ok(BidderResponse::new()); }

        let mut result = BidderResponse::with_capacity(seat_bid.bid.len());
        if let Some(cur) = &bid_resp.cur {
            if !cur.is_empty() { result.currency = cur.clone(); }
        }

        // Build imp map
        let imp_map: HashMap<&str, &openrtb::Imp> = internal.imp.iter()
            .map(|i| (i.id.as_str(), i))
            .collect();

        for bid in seat_bid.bid.clone().iter() {
            let mut bid = bid.clone();
            let imp = imp_map.get(bid.impid.as_str()).copied();

            let bid_type = match get_bid_type(&mut bid, imp) {
                Ok(t) => t,
                Err(e) => return Err(vec![e]),
            };

            // Check for deal ID from bid ext
            if let Some(ext) = &bid.ext {
                if let Ok(bid_ext) = serde_json::from_value::<BidExt>(ext.clone()) {
                    if let Some(id_ext) = bid_ext.improvedigital {
                        if id_ext.line_item_id != 0 && is_deal_buying_type(&id_ext.buying_type) {
                            bid.dealid = Some(id_ext.line_item_id.to_string());
                        }
                    }
                }
            }

            result.bids.push(TypedBid::new(bid, bid_type));
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_req() -> openrtb::BidRequest {
        openrtb::BidRequest {
            id: "r".to_string(),
            imp: vec![openrtb::Imp {
                id: "i1".to_string(),
                banner: Some(openrtb::Banner {
                    w: Some(300),
                    h: Some(250),
                    ..Default::default()
                }),
                ext: Some(serde_json::json!({"bidder": {"placementId": 1}})),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn test_improvedigital_url_and_headers() {
        let adapter =
            ImprovedigitalAdapter::new("https://ad.360yield.com/pbs".to_string());
        let (reqs, _) = adapter.make_requests(&make_req(), &ExtraRequestInfo::default());
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].uri, "https://ad.360yield.com/pbs");
        assert_eq!(
            reqs[0].headers.get("Content-Type").unwrap(),
            "application/json;charset=utf-8"
        );
        assert!(!reqs[0].body.is_empty());
    }

    #[test]
    fn test_improvedigital_make_bids() {
        let adapter =
            ImprovedigitalAdapter::new("https://ad.360yield.com/pbs".to_string());
        let body = br#"{"id":"r","seatbid":[{"bid":[{"id":"b","impid":"i1","price":0.9,"crid":"c","mtype":1}]}]}"#;
        let resp = ResponseData::new(200, body.to_vec());
        let result = adapter
            .make_bids(&make_req(), &RequestData::default(), &resp)
            .unwrap();
        assert_eq!(result.bids.len(), 1);
        assert_eq!(result.bids[0].bid_type, BidType::Banner);
        assert_eq!(result.bids[0].bid.price, 0.9);
    }
}
