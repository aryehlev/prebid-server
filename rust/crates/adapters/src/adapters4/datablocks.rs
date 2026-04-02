use std::collections::HashMap;
use pbs_adapters::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_bid_type_from_imp, get_imp_ids};
use openrtb_ext::BidType;

pub struct DatablocksAdapter {
    pub endpoint: String,
}

impl DatablocksAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

/// Minimal ext structures to extract sourceId from impression bidder params.
#[derive(serde::Deserialize)]
struct ExtImpBidder {
    bidder: DatablocksExt,
}

#[derive(serde::Deserialize, Clone, PartialEq, Eq, Hash)]
struct DatablocksExt {
    #[serde(rename = "sourceId")]
    source_id: i64,
}

impl Bidder for DatablocksAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        // Group impressions by sourceId and create one request per group.
        let mut grouped: HashMap<i64, Vec<openrtb::Imp>> = HashMap::new();
        let mut errors = Vec::new();

        for imp in &request.imp {
            let ext: ExtImpBidder = match imp.ext.as_ref()
                .and_then(|e| serde_json::from_str(e.get()).ok())
            {
                Some(e) => e,
                None => {
                    errors.push(BidderError::BadInput("Missing or invalid bidder ext".to_string()));
                    continue;
                }
            };
            if ext.bidder.source_id < 1 {
                errors.push(BidderError::BadInput("Invalid/Missing SourceId".to_string()));
                continue;
            }
            grouped.entry(ext.bidder.source_id).or_default().push(imp.clone());
        }

        let mut requests = Vec::new();
        for (source_id, imps) in grouped {
            let mut req_copy = request.clone();
            req_copy.imp = imps;

            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => {
                    errors.push(BidderError::BadInput(e.to_string()));
                    continue;
                }
            };

            // Replace the {SourceId} macro in the endpoint template.
            let uri = self.endpoint.replace("{SourceId}", &source_id.to_string());

            let mut headers = HashMap::new();
            headers.insert("Content-Type".to_string(), "application/json".to_string());
            headers.insert("Accept".to_string(), "application/json".to_string());

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
        if let Err(e) = pbs_adapters::check_response_status(response.status_code) {
            return Err(vec![e]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(5);
        if let Some(cur) = &bid_resp.cur {
            result.currency = cur.clone();
        }

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = internal
                    .imp
                    .iter()
                    .find(|i| i.id == bid.impid)
                    .map(get_bid_type_from_imp)
                    .unwrap_or(BidType::Banner);
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        Ok(result)
    }
}
