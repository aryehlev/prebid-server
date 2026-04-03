use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_bid_type_from_imp, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;
use serde_json::Value;

pub struct AdvangelistsAdapter { pub endpoint: String }
impl AdvangelistsAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

#[derive(Deserialize)]
struct ExtImpBidder { bidder: Value }

#[derive(Deserialize, Clone, PartialEq, Eq, Hash)]
struct ExtImpAdvangelists {
    #[serde(rename = "pubid")]
    pub_id: String,
    #[serde(default)]
    placement: String,
}

impl Bidder for AdvangelistsAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::BadInput("No impression in the bid request".to_string())]);
        }
        let mut errs = Vec::new();
        let mut pub2imps: HashMap<String, (ExtImpAdvangelists, Vec<openrtb::Imp>)> = HashMap::new();

        for imp in &request.imp {
            let bidder_ext: ExtImpBidder = match imp.ext.as_ref()
                .and_then(|e| serde_json::from_value(e.clone()).ok()) {
                Some(v) => v,
                None => { errs.push(BidderError::BadInput("failed to parse ext".to_string())); continue; }
            };
            let ext: ExtImpAdvangelists = match serde_json::from_value(bidder_ext.bidder) {
                Ok(v) => v,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };
            if ext.pub_id.is_empty() {
                errs.push(BidderError::BadInput("No pubid value provided".to_string())); continue;
            }
            let mut imp_copy = imp.clone();
            imp_copy.ext = None;
            // Ensure banner has w/h set
            if let Some(banner) = &imp_copy.banner {
                if (banner.w.is_none() || banner.h.is_none()) {
                    let mut b = banner.clone();
                    if let Some(formats) = &b.format {
                        if let Some(first) = formats.first() {
                            b.w = Some(first.w.unwrap_or(0));
                            b.h = Some(first.h.unwrap_or(0));
                            b.format = Some(formats[1..].to_vec());
                        }
                    }
                    imp_copy.banner = Some(b);
                }
            }
            let key = ext.pub_id.clone();
            pub2imps.entry(key).or_insert_with(|| (ext, Vec::new())).1.push(imp_copy);
        }

        if pub2imps.is_empty() { return (vec![], errs); }

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        let mut requests = Vec::new();
        for (_, (ext, imps)) in pub2imps {
            let uri = self.endpoint.replace("{{.PublisherID}}", &ext.pub_id)
                .replace("{{.AdSlot}}", &ext.placement);
            let mut req = request.clone();
            req.imp = imps;
            let body = match serde_json::to_vec(&req) {
                Ok(b) => b,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };
            requests.push(RequestData { method: "POST".to_string(), uri, body, headers: headers.clone(), imp_ids: get_imp_ids(&req.imp) });
        }
        (requests, errs)
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(5);
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = internal.imp.iter().find(|i| i.id == bid.impid).map(get_bid_type_from_imp).unwrap_or(BidType::Banner);
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        Ok(result)
    }
}
