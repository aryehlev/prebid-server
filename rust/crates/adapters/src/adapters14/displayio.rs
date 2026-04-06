use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use openrtb_ext::BidType;

pub struct DisplayioAdapter { pub endpoint: String }
impl DisplayioAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

fn get_bid_type_from_mtype(mtype: i32) -> Result<BidType, BidderError> {
    match mtype {
        1 => Ok(BidType::Banner),
        2 => Ok(BidType::Video),
        _ => Err(BidderError::BadServerResponse(
            format!("unexpected media type for bid with mtype {}", mtype)
        )),
    }
}

impl Bidder for DisplayioAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();
        let mut errs = Vec::new();

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
        headers.insert("x-openrtb-version".to_string(), "2.5".to_string());

        for imp in &request.imp {
            let bidder = imp.ext.as_ref()
                .and_then(|e| e.get("bidder"))
                .cloned()
                .unwrap_or(serde_json::Value::Null);

            if bidder.is_null() {
                errs.push(BidderError::BadInput("impression extensions required".to_string()));
                continue;
            }

            let publisher_id = bidder.get("publisherId").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let placement_id = bidder.get("placementId").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let inventory_id = bidder.get("inventoryId").and_then(|v| v.as_str()).unwrap_or("").to_string();

            // Build URL from template: replace {{.PublisherID}}
            let url = self.endpoint.replace("{{.PublisherID}}", &publisher_id);

            // Build request ext with displayio section
            let mut req_copy = request.clone();
            let mut ext_map: serde_json::Map<String, serde_json::Value> = req_copy.ext.as_ref()
                .and_then(|e| serde_json::from_value(e.clone()).ok())
                .unwrap_or_default();
            ext_map.insert("displayio".to_string(), serde_json::json!({
                "placementId": placement_id,
                "inventoryId": inventory_id
            }));
            req_copy.ext = Some(serde_json::Value::Object(ext_map));
            req_copy.imp = vec![imp.clone()];

            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };

            requests.push(RequestData {
                method: "POST".to_string(),
                uri: url,
                body,
                headers: headers.clone(),
                imp_ids: vec![imp.id.clone()],
            });
        }

        if requests.is_empty() {
            (vec![], errs)
        } else {
            (requests, errs)
        }
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        if bid_resp.seatbid.len() != 1 {
            return Err(vec![BidderError::BadServerResponse(
                format!("Invalid SeatBids count: {}", bid_resp.seatbid.len())
            )]);
        }

        let mut result = BidderResponse::new();
        let mut errors = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let mtype = bid.mtype.unwrap_or(0);
                match get_bid_type_from_mtype(mtype) {
                    Ok(bid_type) => result.bids.push(TypedBid::new(bid, bid_type)),
                    Err(e) => errors.push(e),
                }
            }
        }

        if !errors.is_empty() {
            return Err(errors);
        }
        Ok(result)
    }
}
