use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids, check_response_status};
use openrtb::BidResponse;
use openrtb_ext::BidType;

pub struct OwnadxAdapter { pub endpoint: String }
impl OwnadxAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct OwnAdxExt {
    ssp_id: String,
    seat_id: String,
    token_id: String,
}

fn get_imp_ext(imp: &openrtb::Imp) -> Result<OwnAdxExt, BidderError> {
    let bidder = imp.ext.as_ref()
        .and_then(|e| e.get("bidder"))
        .ok_or_else(|| BidderError::BadInput("Bidder extension not valid or can't be unmarshalled".to_string()))?;
    Ok(OwnAdxExt {
        ssp_id: bidder.get("sspId").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        seat_id: bidder.get("seatId").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        token_id: bidder.get("tokenId").and_then(|v| v.as_str()).unwrap_or("").to_string(),
    })
}

fn build_endpoint_url(template: &str, ext: &OwnAdxExt) -> String {
    template
        .replace("{{.SspID}}", &ext.ssp_id)
        .replace("{{.SeatID}}", &ext.seat_id)
        .replace("{{.TokenID}}", &ext.token_id)
}

fn get_bid_type_from_mtype(mtype: u64) -> Result<BidType, BidderError> {
    match mtype {
        1 => Ok(BidType::Banner),
        2 => Ok(BidType::Video),
        3 => Ok(BidType::Audio),
        4 => Ok(BidType::Native),
        _ => Err(BidderError::BadServerResponse("Bid type is invalid".to_string())),
    }
}

impl Bidder for OwnadxAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::BadInput("No impression in the bid request".to_string())]);
        }

        let mut errs = Vec::new();
        let mut groups: HashMap<OwnAdxExt, Vec<openrtb::Imp>> = HashMap::new();

        for imp in &request.imp {
            match get_imp_ext(imp) {
                Ok(ext) => groups.entry(ext).or_default().push(imp.clone()),
                Err(e) => errs.push(e),
            }
        }

        if groups.is_empty() {
            return (vec![], errs);
        }

        let mut requests = Vec::new();
        for (ext, imps) in groups {
            let url = build_endpoint_url(&self.endpoint, &ext);
            let mut req_copy = request.clone();
            req_copy.imp = imps;
            let imp_ids = get_imp_ids(&req_copy.imp);
            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };
            let mut headers = HashMap::new();
            headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
            headers.insert("Accept".to_string(), "application/json".to_string());
            headers.insert("x-openrtb-version".to_string(), "2.5".to_string());
            requests.push(RequestData {
                method: "POST".to_string(),
                uri: url,
                body,
                headers,
                imp_ids,
            });
        }

        (requests, errs)
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        if bid_resp.seatbid.is_empty() {
            return Err(vec![BidderError::BadServerResponse("Array SeatBid cannot be empty".to_string())]);
        }
        let seat_bid = &bid_resp.seatbid[0];
        if seat_bid.bid.is_empty() {
            return Err(vec![BidderError::BadServerResponse("Bid cannot be empty".to_string())]);
        }
        let mut result = BidderResponse::with_capacity(seat_bid.bid.len());
        for bid in bid_resp.seatbid.into_iter().flat_map(|sb| sb.bid) {
            let mtype = bid.ext.as_ref()
                .and_then(|e| e.get("mtype"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let bid_type = get_bid_type_from_mtype(mtype)
                .map_err(|e| vec![e])?;
            result.bids.push(TypedBid::new(bid, bid_type));
        }
        Ok(result)
    }
}
