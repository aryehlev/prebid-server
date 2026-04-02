use std::collections::HashMap;
use pbs_adapters::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct MinutemediaAdapter {
    pub endpoint: String,
}

impl MinutemediaAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

/// Extract the `org` field from the first impression's bidder extension.
fn extract_org(request: &openrtb::BidRequest) -> Result<String, BidderError> {
    let imp = request
        .imp
        .first()
        .ok_or_else(|| BidderError::BadInput("no imps in bid request".to_string()))?;

    let ext = imp
        .ext
        .as_ref()
        .ok_or_else(|| BidderError::BadInput("imp.ext is missing".to_string()))?;

    // ext is a raw JSON value; extract bidder.org
    let bidder = ext
        .get("bidder")
        .ok_or_else(|| BidderError::BadInput("imp.ext.bidder is missing".to_string()))?;

    let org = bidder
        .get("org")
        .and_then(|v| v.as_str())
        .ok_or_else(|| BidderError::BadInput("imp.ext.bidder.org is missing".to_string()))?
        .trim()
        .to_string();

    Ok(org)
}

/// Determine bid type from MType field (1=banner, 2=video).
fn get_bid_type(mtype: Option<i32>) -> Result<BidType, BidderError> {
    match mtype {
        Some(1) => Ok(BidType::Banner),
        Some(2) => Ok(BidType::Video),
        other => Err(BidderError::BadServerResponse(format!(
            "unsupported MType {:?}",
            other
        ))),
    }
}

impl Bidder for MinutemediaAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let org = match extract_org(request) {
            Ok(o) => o,
            Err(e) => return (vec![], vec![e]),
        };

        let body = match serde_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        // Use test endpoint when request.test == 1
        let base_url = if request.test == Some(1) {
            "https://pbs.minutemedia-prebid.com/pbs-test".to_string()
        } else {
            self.endpoint.clone()
        };

        let uri = format!(
            "{}?publisher_id={}",
            base_url,
            urlencoding::encode(&org)
        );

        let mut headers = HashMap::new();
        headers.insert(
            "Content-Type".to_string(),
            "application/json;charset=utf-8".to_string(),
        );

        (
            vec![RequestData {
                method: "POST".to_string(),
                uri,
                body,
                headers,
                imp_ids: get_imp_ids(&request.imp),
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
        if let Err(e) = pbs_adapters::check_response_status(response.status_code) {
            return Err(vec![e]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(5);
        if let Some(cur) = &bid_resp.cur {
            result.currency = cur.clone();
        }

        let mut errors = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                match get_bid_type(bid.mtype) {
                    Ok(bid_type) => result.bids.push(TypedBid::new(bid, bid_type)),
                    Err(e) => errors.push(e),
                }
            }
        }

        if !errors.is_empty() && result.bids.is_empty() {
            return Err(errors);
        }

        Ok(result)
    }
}
