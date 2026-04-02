use std::collections::HashMap;
use pbs_adapters::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;

pub struct MgidAdapter {
    pub endpoint: String,
}

impl MgidAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

#[derive(Deserialize)]
struct MgidImpExt {
    #[serde(rename = "accountId", default)]
    account_id: String,
    #[serde(rename = "placementId", default)]
    placement_id: String,
    #[serde(rename = "currency", default)]
    currency: String,
    #[serde(rename = "cur", default)]
    cur: String,
    #[serde(rename = "bidfloor", default)]
    bid_floor: f64,
    #[serde(rename = "bidfloor2", default)]
    bid_floor2: f64,
}

#[derive(Deserialize)]
struct RespBidExt {
    #[serde(rename = "crtype")]
    crtype: Option<String>,
}

impl Bidder for MgidAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut req_copy = request.clone();

        // Default TMax to 200 if not set
        if req_copy.tmax.is_none() || req_copy.tmax == Some(0) {
            req_copy.tmax = Some(200);
        }

        let mut path = String::new();

        for imp in req_copy.imp.iter_mut() {
            let ext = match imp.ext.as_ref() {
                Some(e) => e.clone(),
                None => {
                    return (vec![], vec![BidderError::BadInput("imp.ext missing".to_string())]);
                }
            };

            let bidder_val = match ext.get("bidder") {
                Some(v) => v.clone(),
                None => {
                    return (
                        vec![],
                        vec![BidderError::BadInput("imp.ext.bidder missing".to_string())],
                    );
                }
            };

            let mgid_ext: MgidImpExt = match serde_json::from_value(bidder_val) {
                Ok(v) => v,
                Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
            };

            if path.is_empty() {
                path = mgid_ext.account_id.clone();
            }

            // Build tag ID
            if mgid_ext.placement_id.is_empty() {
                imp.tagid = Some(imp.id.clone());
            } else {
                imp.tagid = Some(format!("{}/{}", mgid_ext.placement_id, imp.id));
            }

            // Currency
            let cur = if !mgid_ext.currency.is_empty() && mgid_ext.currency != "USD" {
                mgid_ext.currency.clone()
            } else if !mgid_ext.cur.is_empty() && mgid_ext.cur != "USD" {
                mgid_ext.cur.clone()
            } else {
                String::new()
            };

            // Bid floor
            let bid_floor = if mgid_ext.bid_floor > 0.0 {
                mgid_ext.bid_floor
            } else if mgid_ext.bid_floor2 > 0.0 {
                mgid_ext.bid_floor2
            } else {
                0.0
            };

            if bid_floor > 0.0 {
                imp.bidfloor = Some(bid_floor);
            }
            if !cur.is_empty() {
                imp.bidfloorcur = Some(cur);
            }
        }

        if path.is_empty() {
            return (
                vec![],
                vec![BidderError::BadInput("accountId is not set".to_string())],
            );
        }

        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let mut headers = HashMap::new();
        headers.insert(
            "Content-Type".to_string(),
            "application/json;charset=utf-8".to_string(),
        );
        headers.insert("Accept".to_string(), "application/json".to_string());

        (
            vec![RequestData {
                method: "POST".to_string(),
                uri: format!("{}{}", self.endpoint, path),
                body,
                headers,
                imp_ids: get_imp_ids(&req_copy.imp),
            }],
            vec![],
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

        let mut result = BidderResponse::with_capacity(5);
        if let Some(cur) = &bid_resp.cur {
            result.currency = cur.clone();
        }

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                // Determine bid type from crtype in bid.ext
                let bid_type = bid
                    .ext
                    .as_ref()
                    .and_then(|ext| serde_json::from_value::<RespBidExt>(ext.clone()).ok())
                    .and_then(|ext| ext.crtype)
                    .and_then(|ct| match ct.as_str() {
                        "banner" => Some(BidType::Banner),
                        "video" => Some(BidType::Video),
                        "native" => Some(BidType::Native),
                        _ => None,
                    })
                    .unwrap_or(BidType::Banner);

                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }

        Ok(result)
    }
}
