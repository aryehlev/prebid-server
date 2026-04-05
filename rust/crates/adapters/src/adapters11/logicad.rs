use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb::BidResponse;
use openrtb_ext::BidType;

pub struct LogicadAdapter { pub endpoint: String }
impl LogicadAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

impl Bidder for LogicadAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::BadInput("No impression in the bid request".to_string())]);
        }

        let mut errs = Vec::new();
        let mut requests = Vec::new();

        // Group imps by tid (from imp.ext.bidder.tid)
        let mut groups: HashMap<String, Vec<openrtb::Imp>> = HashMap::new();

        for imp in &request.imp {
            let tid = imp.ext.as_ref()
                .and_then(|e| e.get("bidder"))
                .and_then(|b| b.get("tid"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            if tid.is_empty() {
                errs.push(BidderError::BadInput("No tid value provided".to_string()));
                continue;
            }

            groups.entry(tid).or_default().push(imp.clone());
        }

        if groups.is_empty() {
            return (vec![], errs);
        }

        for (tid, imps) in groups {
            let imp_ids = get_imp_ids(&imps);
            // Build modified request: set TagID = tid on each imp, clear ext
            let modified_imps: Vec<openrtb::Imp> = imps.into_iter().map(|mut i| {
                i.tagid = Some(tid.clone());
                i.ext = None;
                i
            }).collect();

            let mut req_copy = request.clone();
            req_copy.imp = modified_imps;

            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(BidderError::BadInput(e.to_string()));
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
                imp_ids,
            });
        }

        (requests, errs)
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(
                format!("Unexpected http status code: {}", response.status_code)
            )]);
        }
        let bid_resp: BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(format!("Bad server response: {}", e))])?;
        if bid_resp.seatbid.len() != 1 {
            return Err(vec![BidderError::BadServerResponse(
                format!("Invalid SeatBids count: {}", bid_resp.seatbid.len())
            )]);
        }
        let seatbid = &bid_resp.seatbid[0];
        let mut result = BidderResponse::with_capacity(seatbid.bid.len());
        if let Some(cur) = bid_resp.cur.filter(|c| !c.is_empty()) {
            result.currency = cur;
        }
        for bid in seatbid.bid.iter().cloned() {
            result.bids.push(TypedBid::new(bid, BidType::Banner));
        }
        Ok(result)
    }
}
