use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use openrtb_ext::BidType;
use serde::{Deserialize, Serialize};

pub struct ContxtfulAdapter { pub endpoint: String }
impl ContxtfulAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

#[derive(Debug, Deserialize, Clone)]
struct ExtImpContxtful {
    #[serde(rename = "placementId", default)]
    placement_id: String,
    #[serde(rename = "customerId", default)]
    customer_id: String,
}

#[derive(Debug, Serialize, Clone)]
struct ContxtfulRequestPayload {
    ortb2: openrtb::BidRequest,
    #[serde(rename = "bidRequests")]
    bid_requests: Vec<ContxtfulBidRequest>,
    #[serde(rename = "bidderRequest")]
    bidder_request: ContxtfulBidderRequest,
    config: ContxtfulConfig,
}

#[derive(Debug, Serialize, Clone)]
struct ContxtfulBidRequest {
    bidder: String,
    params: ContxtfulBidRequestParams,
    #[serde(rename = "bidId")]
    bid_id: String,
}

#[derive(Debug, Serialize, Clone)]
struct ContxtfulBidRequestParams {
    #[serde(rename = "placementId")]
    placement_id: String,
}

#[derive(Debug, Serialize, Clone)]
struct ContxtfulBidderRequest {
    #[serde(rename = "bidderCode")]
    bidder_code: String,
}

#[derive(Debug, Serialize, Clone)]
struct ContxtfulConfig {
    contxtful: ContxtfulConfigDetails,
}

#[derive(Debug, Serialize, Clone)]
struct ContxtfulConfigDetails {
    version: String,
    customer: String,
}

#[derive(Debug, Deserialize, Clone)]
struct ContxtfulExchangeBid {
    #[serde(rename = "requestId", default)]
    request_id: String,
    #[serde(default)]
    cpm: f64,
    #[serde(default)]
    currency: String,
    #[serde(default)]
    width: i64,
    #[serde(default)]
    height: i64,
    #[serde(rename = "creativeId", default)]
    creative_id: String,
    #[serde(default)]
    adm: String,
    #[serde(rename = "mediaType", default)]
    media_type: String,
    #[serde(default)]
    nurl: String,
    #[serde(default)]
    burl: String,
    #[serde(default)]
    lurl: String,
}

impl Bidder for ContxtfulAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut valid_placements: Vec<String> = Vec::new();
        let mut customer_id = String::new();

        for imp in &request.imp {
            let bidder_val = match imp.ext.as_ref().and_then(|e| e.get("bidder")) {
                Some(v) => v.clone(),
                None => {
                    errs.push(BidderError::BadInput("missing bidder ext".to_string()));
                    continue;
                }
            };
            let params: ExtImpContxtful = match serde_json::from_value(bidder_val) {
                Ok(p) => p,
                Err(e) => {
                    errs.push(BidderError::BadInput(e.to_string()));
                    continue;
                }
            };
            if customer_id.is_empty() {
                customer_id = params.customer_id.clone();
            }
            valid_placements.push(params.placement_id);
        }

        if valid_placements.is_empty() {
            return (vec![], errs);
        }

        // Build endpoint URL: replace {{.AccountID}} with customer_id
        let endpoint = self.endpoint.replace("{{.AccountID}}", &customer_id);

        // Build bid requests - zip with placements
        let bid_requests: Vec<ContxtfulBidRequest> = request.imp.iter().zip(valid_placements.iter())
            .map(|(imp, placement)| ContxtfulBidRequest {
                bidder: "contxtful".to_string(),
                params: ContxtfulBidRequestParams { placement_id: placement.clone() },
                bid_id: imp.id.clone(),
            })
            .collect();

        let payload = ContxtfulRequestPayload {
            ortb2: request.clone(),
            bid_requests,
            bidder_request: ContxtfulBidderRequest { bidder_code: "contxtful".to_string() },
            config: ContxtfulConfig {
                contxtful: ContxtfulConfigDetails {
                    version: "v1".to_string(),
                    customer: customer_id,
                },
            },
        };

        let body = match serde_json::to_vec(&payload) {
            Ok(b) => b,
            Err(e) => {
                errs.push(BidderError::BadInput(e.to_string()));
                return (vec![], errs);
            }
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        let imp_ids: Vec<String> = request.imp.iter().map(|i| i.id.clone()).collect();
        (vec![RequestData { method: "POST".to_string(), uri: endpoint, body, headers, imp_ids }], errs)
    }

    fn make_bids(&self, request: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }

        // Try to parse as array of ContxtfulExchangeBid (PrebidJS format)
        let prebid_bids: Vec<ContxtfulExchangeBid> = match serde_json::from_slice(&response.body) {
            Ok(b) => b,
            Err(_) => return Err(vec![BidderError::BadServerResponse(
                "failed to parse response as Contxtful relay or Prebid.js format".to_string()
            )]),
        };

        if prebid_bids.is_empty() {
            return Err(vec![BidderError::BadServerResponse(
                "failed to parse response as Contxtful relay or Prebid.js format".to_string()
            )]);
        }

        let mut result = BidderResponse::new();
        let mut errs = Vec::new();

        for prebid_bid in &prebid_bids {
            if prebid_bid.cpm == 0.0 || prebid_bid.request_id.is_empty() {
                continue;
            }
            if prebid_bid.media_type.is_empty() {
                errs.push(BidderError::BadServerResponse("bid has no ad media type".to_string()));
                continue;
            }
            if prebid_bid.adm.is_empty() {
                errs.push(BidderError::BadServerResponse("bid has no ad markup".to_string()));
                continue;
            }

            let bid_type = request.imp.iter()
                .find(|i| i.id == prebid_bid.request_id)
                .map(|imp| {
                    if imp.video.is_some() { BidType::Video }
                    else if imp.native.is_some() { BidType::Native }
                    else { BidType::Banner }
                })
                .unwrap_or(BidType::Banner);

            let bid = openrtb::Bid {
                id: format!("contxtful-{}", prebid_bid.request_id),
                impid: prebid_bid.request_id.clone(),
                price: prebid_bid.cpm,
                adm: Some(prebid_bid.adm.clone()),
                w: Some(prebid_bid.width as i32),
                h: Some(prebid_bid.height as i32),
                crid: Some(prebid_bid.creative_id.clone()),
                nurl: if prebid_bid.nurl.is_empty() { None } else { Some(prebid_bid.nurl.clone()) },
                burl: if prebid_bid.burl.is_empty() { None } else { Some(prebid_bid.burl.clone()) },
                lurl: if prebid_bid.lurl.is_empty() { None } else { Some(prebid_bid.lurl.clone()) },
                ..Default::default()
            };

            if !prebid_bid.currency.is_empty() {
                result.currency = prebid_bid.currency.clone();
            }
            result.bids.push(TypedBid::new(bid, bid_type));
        }

        if result.bids.is_empty() && !errs.is_empty() {
            Err(errs)
        } else {
            Ok(result)
        }
    }
}
