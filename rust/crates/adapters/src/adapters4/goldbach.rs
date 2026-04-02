use std::collections::HashMap;
use pbs_adapters::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct GoldbachAdapter {
    pub endpoint: String,
}

impl GoldbachAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

#[derive(serde::Deserialize)]
struct ExtImpBidder {
    bidder: GoldbachImpExt,
}

#[derive(serde::Deserialize)]
struct GoldbachImpExt {
    #[serde(rename = "publisherId")]
    publisher_id: String,
    #[serde(rename = "slotId")]
    slot_id: String,
    #[serde(rename = "customTargeting", default)]
    custom_targeting: HashMap<String, Vec<String>>,
}

#[derive(serde::Serialize)]
struct OutgoingImpExt {
    goldbach: OutgoingGoldbachImpExt,
}

#[derive(serde::Serialize)]
struct OutgoingGoldbachImpExt {
    #[serde(rename = "slotId")]
    slot_id: String,
    #[serde(rename = "targetings", skip_serializing_if = "HashMap::is_empty")]
    targetings: HashMap<String, Vec<String>>,
}

#[derive(serde::Serialize)]
struct OutgoingReqExt {
    goldbach: OutgoingGoldbachReqExt,
}

#[derive(serde::Serialize)]
struct OutgoingGoldbachReqExt {
    #[serde(rename = "publisherId")]
    publisher_id: String,
}

/// Bid ext to extract prebid type.
#[derive(serde::Deserialize, Default)]
struct BidExtPrebid {
    #[serde(rename = "type", default)]
    bid_type: String,
}

#[derive(serde::Deserialize, Default)]
struct BidExt {
    #[serde(default)]
    prebid: BidExtPrebid,
}

fn get_bid_type_from_ext(bid: &openrtb::Bid) -> Result<BidType, BidderError> {
    let ext_val = bid.ext.as_ref().ok_or_else(|| {
        BidderError::BadServerResponse(format!("no media type for bid {}", bid.id))
    })?;

    let bid_ext: BidExt = serde_json::from_str(ext_val.get())
        .map_err(|e| BidderError::BadServerResponse(e.to_string()))?;

    if bid_ext.prebid.bid_type.is_empty() {
        return Err(BidderError::BadServerResponse(format!(
            "no media type for bid {}",
            bid.id
        )));
    }

    match bid_ext.prebid.bid_type.as_str() {
        "banner" => Ok(BidType::Banner),
        "video" => Ok(BidType::Video),
        "native" => Ok(BidType::Native),
        "audio" => Ok(BidType::Audio),
        other => Err(BidderError::BadServerResponse(format!(
            "unsupported bid type: {}",
            other
        ))),
    }
}

impl Bidder for GoldbachAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errors = Vec::new();

        // Group impressions by publisher_id.
        let mut publisher_imps: HashMap<String, Vec<openrtb::Imp>> = HashMap::new();

        for imp in &request.imp {
            let imp_ext = match imp.ext.as_ref()
                .and_then(|e| serde_json::from_str::<ExtImpBidder>(e.get()).ok())
            {
                Some(e) => e.bidder,
                None => {
                    errors.push(BidderError::BadInput(
                        "unable to unmarshal imp.ext.bidder".to_string(),
                    ));
                    continue;
                }
            };

            if imp_ext.publisher_id.is_empty() || imp_ext.slot_id.is_empty() {
                errors.push(BidderError::BadInput(
                    "publisherId and slotId are required".to_string(),
                ));
                continue;
            }

            let outgoing_ext = OutgoingImpExt {
                goldbach: OutgoingGoldbachImpExt {
                    slot_id: imp_ext.slot_id,
                    targetings: imp_ext.custom_targeting,
                },
            };

            let new_ext = match serde_json::to_value(&outgoing_ext) {
                Ok(v) => v,
                Err(e) => {
                    errors.push(BidderError::BadInput(e.to_string()));
                    continue;
                }
            };

            let mut imp_copy = imp.clone();
            imp_copy.ext = Some(new_ext);

            publisher_imps
                .entry(imp_ext.publisher_id)
                .or_default()
                .push(imp_copy);
        }

        if publisher_imps.is_empty() {
            errors.push(BidderError::BadInput("no valid impression found".to_string()));
            return (vec![], errors);
        }

        let mut requests = Vec::new();

        for (publisher_id, imps) in publisher_imps {
            let req_ext = OutgoingReqExt {
                goldbach: OutgoingGoldbachReqExt {
                    publisher_id: publisher_id.clone(),
                },
            };

            let req_ext_val = match serde_json::to_value(&req_ext) {
                Ok(v) => v,
                Err(e) => {
                    errors.push(BidderError::BadInput(e.to_string()));
                    continue;
                }
            };

            let mut req_copy = request.clone();
            req_copy.imp = imps;
            req_copy.id = format!("{}_{}", request.id, publisher_id);
            req_copy.ext = Some(req_ext_val);

            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => {
                    errors.push(BidderError::BadInput(e.to_string()));
                    continue;
                }
            };

            let mut headers = HashMap::new();
            headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
            headers.insert("Accept".to_string(), "application/json".to_string());

            requests.push(RequestData {
                method: "POST".to_string(),
                uri: self.endpoint.clone(),
                body,
                headers,
                imp_ids: get_imp_ids(&req_copy.imp),
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
        // Goldbach uses 201 Created for successful responses.
        if response.status_code != 201 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "unexpected status code: {}. Run with request.debug = 1 for more info",
                response.status_code
            ))]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::new();
        if let Some(cur) = &bid_resp.cur {
            result.currency = cur.clone();
        }

        let mut errors = Vec::new();

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                match get_bid_type_from_ext(&bid) {
                    Ok(bid_type) => result.bids.push(TypedBid::new(bid, bid_type)),
                    Err(e) => errors.push(e),
                }
            }
        }

        if result.bids.is_empty() {
            errors.push(BidderError::BadServerResponse(
                "no valid bids found in response".to_string(),
            ));
            return Err(errors);
        }

        Ok(result)
    }
}
