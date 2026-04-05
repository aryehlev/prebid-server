use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::{Deserialize, Serialize};

pub struct GoldbachAdapter { pub endpoint: String }
impl GoldbachAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

#[derive(Debug, Default, Deserialize)]
struct ExtImpGoldbach {
    #[serde(rename = "publisherId", default)]
    publisher_id: String,
    #[serde(rename = "slotId", default)]
    slot_id: String,
    #[serde(rename = "customTargeting", default)]
    custom_targeting: HashMap<String, Vec<String>>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct ImpExtGoldbachOutgoing {
    #[serde(rename = "targetings", skip_serializing_if = "HashMap::is_empty")]
    targetings: HashMap<String, Vec<String>>,
    #[serde(rename = "slotId")]
    slot_id: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct ImpExtAdapter {
    #[serde(rename = "goldbach")]
    goldbach: ImpExtGoldbachOutgoing,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct RequestExtGoldbach {
    #[serde(rename = "publisherId")]
    publisher_id: String,
    #[serde(rename = "mockResponse", skip_serializing_if = "Option::is_none")]
    mock_response: Option<bool>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct RequestExtAdapter {
    #[serde(rename = "goldbach")]
    goldbach: RequestExtGoldbach,
}

fn extract_imp_ext(imp: &openrtb::Imp) -> Result<ExtImpGoldbach, BidderError> {
    let bidder_val = imp.ext.as_ref()
        .and_then(|e| e.get("bidder"))
        .cloned()
        .ok_or_else(|| BidderError::BadInput(format!("unable to unmarshal imp.ext: missing bidder")))?;
    let goldbach_ext: ExtImpGoldbach = serde_json::from_value(bidder_val)
        .map_err(|e| BidderError::BadInput(format!("unable to unmarshal imp.ext.bidder: {}", e)))?;
    if goldbach_ext.publisher_id.is_empty() || goldbach_ext.slot_id.is_empty() {
        return Err(BidderError::BadInput("publisherId and slotId are required".to_string()));
    }
    Ok(goldbach_ext)
}

fn build_imp(imp: &openrtb::Imp) -> Result<(String, openrtb::Imp), BidderError> {
    let goldbach_ext = extract_imp_ext(imp)?;

    let mut targetings = HashMap::new();
    for (k, v) in &goldbach_ext.custom_targeting {
        targetings.insert(k.clone(), v.clone());
    }

    let imp_ext_val = serde_json::to_value(&ImpExtAdapter {
        goldbach: ImpExtGoldbachOutgoing {
            targetings,
            slot_id: goldbach_ext.slot_id.clone(),
        },
    }).map_err(|e| BidderError::BadInput(format!("unable to marshal imp.ext: {}", e)))?;

    let mut imp_copy = imp.clone();
    imp_copy.ext = Some(imp_ext_val);

    Ok((goldbach_ext.publisher_id, imp_copy))
}

fn get_bid_type_from_ext(bid: &openrtb::Bid) -> Result<BidType, BidderError> {
    let ext = bid.ext.as_ref()
        .ok_or_else(|| BidderError::BadInput(format!("no media type for bid {}", bid.id)))?;

    let prebid = ext.get("prebid")
        .ok_or_else(|| BidderError::BadInput(format!("no media type for bid {}", bid.id)))?;

    let type_str = prebid.get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if type_str.is_empty() {
        return Err(BidderError::BadInput(format!("no media type for bid {}", bid.id)));
    }

    match type_str {
        "video" => Ok(BidType::Video),
        "native" => Ok(BidType::Native),
        "audio" => Ok(BidType::Audio),
        _ => Ok(BidType::Banner),
    }
}

impl Bidder for GoldbachAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();

        // Parse existing request.ext to extract goldbach fields (e.g. mockResponse)
        let existing_goldbach_ext: RequestExtGoldbach = request.ext.as_ref()
            .and_then(|e| e.get("goldbach"))
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        // Group impressions by publisher ID
        let mut publisher_imps: HashMap<String, Vec<openrtb::Imp>> = HashMap::new();
        for imp in &request.imp {
            match build_imp(imp) {
                Ok((pub_id, imp_copy)) => {
                    publisher_imps.entry(pub_id).or_default().push(imp_copy);
                }
                Err(e) => errs.push(e),
            }
        }

        if publisher_imps.is_empty() {
            errs.push(BidderError::BadInput("no valid impression found".to_string()));
            return (vec![], errs);
        }

        let mut reqs = Vec::new();
        for (pub_id, imps) in publisher_imps {
            let mut req_copy = request.clone();
            req_copy.imp = imps;
            req_copy.id = format!("{}_{}", request.id, pub_id);

            // Build request ext preserving mockResponse from the original ext
            let ext_val = serde_json::to_value(&RequestExtAdapter {
                goldbach: RequestExtGoldbach {
                    publisher_id: pub_id.clone(),
                    mock_response: existing_goldbach_ext.mock_response,
                },
            });
            match ext_val {
                Ok(v) => req_copy.ext = Some(v),
                Err(e) => {
                    errs.push(BidderError::BadInput(format!("unable to marshal request.ext: {}", e)));
                    continue;
                }
            }

            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(BidderError::BadInput(format!("unable to marshal request: {}", e)));
                    continue;
                }
            };

            let mut headers = HashMap::new();
            headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
            headers.insert("Accept".to_string(), "application/json".to_string());

            let imp_ids = get_imp_ids(&req_copy.imp);
            reqs.push(RequestData {
                method: "POST".to_string(),
                uri: self.endpoint.clone(),
                body,
                headers,
                imp_ids,
            });
        }

        (reqs, errs)
    }

    fn make_bids(&self, _internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code != 201 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "unexpected status code: {}. Run with request.debug = 1 for more info", response.status_code
            ))]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(format!("unable to unmarshal response: {}", e))])?;

        let mut result = BidderResponse::new();
        if let Some(cur) = &bid_resp.cur {
            result.currency = cur.clone();
        }

        let mut errs = Vec::new();
        for sb in &bid_resp.seatbid {
            for bid in &sb.bid {
                match get_bid_type_from_ext(bid) {
                    Ok(bid_type) => result.bids.push(TypedBid::new(bid.clone(), bid_type)),
                    Err(e) => errs.push(e),
                }
            }
        }

        if result.bids.is_empty() {
            errs.push(BidderError::BadServerResponse("no valid bids found in response".to_string()));
            return Err(errs);
        }

        Ok(result)
    }
}
