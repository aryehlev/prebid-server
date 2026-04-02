use std::collections::HashMap;
use pbs_adapters::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct VidazooAdapter {
    pub endpoint: String,
}

impl VidazooAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

fn get_bid_type_from_mtype(mtype: u32, imp_id: &str) -> Result<BidType, BidderError> {
    match mtype {
        1 => Ok(BidType::Banner),
        2 => Ok(BidType::Video),
        _ => Err(BidderError::BadInput(format!(
            "Could not define bid type for imp: {}", imp_id
        ))),
    }
}

fn extract_cid(imp: &openrtb::Imp) -> Option<String> {
    imp.ext.as_ref()
        .and_then(|e| e.get("bidder"))
        .and_then(|b| b.get("cId").or_else(|| b.get("connectionId")))
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
}

impl Bidder for VidazooAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();
        let mut errs = Vec::new();

        for imp in &request.imp {
            let cid = match extract_cid(imp) {
                Some(c) if !c.is_empty() => c,
                _ => {
                    errs.push(BidderError::BadInput(format!(
                        "extract cId: missing or empty connectionId for imp {}", imp.id
                    )));
                    continue;
                }
            };

            let mut req = request.clone();
            req.imp = vec![imp.clone()];

            let body = match serde_json::to_vec(&req) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(BidderError::BadInput(format!("marshal bidRequest: {}", e)));
                    continue;
                }
            };

            let mut headers = HashMap::new();
            headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());

            // Append url-encoded cid to endpoint
            let encoded_cid: String = cid.chars().map(|c| match c {
                'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
                c => format!("%{:02X}", c as u32),
            }).collect();

            let uri = format!("{}{}", self.endpoint, encoded_cid);

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
            return Err(vec![BidderError::BadInput(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info",
                response.status_code
            ))]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(format!("bad server response: {}. ", e))])?;

        let mut result = BidderResponse::with_capacity(bid_resp.seatbid.len());

        if let Some(cur) = &bid_resp.cur {
            if !cur.is_empty() {
                result.currency = cur.clone();
            }
        }

        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let mtype = bid.ext.as_ref()
                    .and_then(|e| e.get("mtype"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32;
                match get_bid_type_from_mtype(mtype, &bid.impid) {
                    Ok(bid_type) => result.bids.push(TypedBid::new(bid, bid_type)),
                    Err(e) => errs.push(e),
                }
            }
        }

        Ok(result)
    }
}
