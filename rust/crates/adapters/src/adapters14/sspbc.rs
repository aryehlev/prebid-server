use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Serialize;

pub struct SspBcAdapter { pub endpoint: String }
impl SspBcAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

#[derive(Serialize)]
struct SspBcRequestInfo {
    #[serde(rename = "pbsEntryPoint")]
    pbs_entry_point: String,
}

#[derive(Serialize)]
struct SspBcRequest<'a> {
    #[serde(rename = "bidRequest")]
    bid_request: &'a openrtb::BidRequest,
    #[serde(rename = "requestInfo")]
    request_info: SspBcRequestInfo,
}

fn get_bid_type_from_mtype(mtype: i32, _imp_id: &str) -> Result<BidType, BidderError> {
    match mtype {
        1 => Ok(BidType::Banner),
        2 => Ok(BidType::Video),
        3 => Ok(BidType::Audio),
        4 => Ok(BidType::Native),
        _ => Err(BidderError::BadServerResponse(format!("unsupported MType: {}.", mtype))),
    }
}

impl Bidder for SspBcAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, extra: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        // Build endpoint with bdver query param
        let endpoint = if self.endpoint.contains('?') {
            format!("{}&bdver=6.0", self.endpoint)
        } else {
            format!("{}?bdver=6.0", self.endpoint)
        };

        let wrapper = SspBcRequest {
            bid_request: request,
            request_info: SspBcRequestInfo {
                pbs_entry_point: extra.pbs_entry_point.clone(),
            },
        };
        let body = match serde_json::to_vec(&wrapper) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };
        (vec![RequestData {
            method: "POST".to_string(),
            uri: endpoint,
            body,
            headers: HashMap::new(),
            imp_ids: get_imp_ids(&request.imp),
        }], vec![])
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!("unexpected status code: {}.", response.status_code))]);
        }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(5);
        if let Some(cur) = &bid_resp.cur {
            if !cur.is_empty() { result.currency = cur.clone(); }
        }
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let mtype = bid.mtype.unwrap_or(0);
                match get_bid_type_from_mtype(mtype, &bid.impid) {
                    Ok(t) => result.bids.push(TypedBid::new(bid, t)),
                    Err(e) => return Err(vec![e]),
                }
            }
        }
        Ok(result)
    }
}
