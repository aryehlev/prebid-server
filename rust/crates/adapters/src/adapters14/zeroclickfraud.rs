use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;

pub struct ZeroclickfraudAdapter { pub endpoint: String }
impl ZeroclickfraudAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

#[derive(Debug, Deserialize, Default, PartialEq, Eq, Hash, Clone)]
struct ExtImpZeroClickFraud {
    #[serde(default)]
    host: String,
    #[serde(rename = "sourceId", default)]
    source_id: i64,
}

fn get_media_type(imp_id: &str, imps: &[openrtb::Imp]) -> BidType {
    for imp in imps {
        if imp.id == imp_id {
            if imp.video.is_some() { return BidType::Video; }
            if imp.native.is_some() { return BidType::Native; }
            return BidType::Banner;
        }
    }
    BidType::Banner
}

impl Bidder for ZeroclickfraudAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut imp_groups: HashMap<ExtImpZeroClickFraud, Vec<openrtb::Imp>> = HashMap::new();

        for imp in &request.imp {
            let bidder_val = match imp.ext.as_ref().and_then(|e| e.get("bidder")) {
                Some(v) => v.clone(),
                None => {
                    errs.push(BidderError::BadInput("Missing bidder ext: missing bidder".to_string()));
                    return (vec![], errs);
                }
            };
            let zcf_ext: ExtImpZeroClickFraud = match serde_json::from_value(bidder_val) {
                Ok(e) => e,
                Err(e) => {
                    errs.push(BidderError::BadInput(format!("Cannot Resolve host or sourceId: {}", e)));
                    return (vec![], errs);
                }
            };

            if zcf_ext.source_id < 1 {
                errs.push(BidderError::BadInput("Invalid/Missing SourceId".to_string()));
                return (vec![], errs);
            }
            if zcf_ext.host.is_empty() {
                errs.push(BidderError::BadInput("Invalid/Missing Host".to_string()));
                return (vec![], errs);
            }

            imp_groups.entry(zcf_ext).or_default().push(imp.clone());
        }

        let headers = {
            let mut h = HashMap::new();
            h.insert("Content-Type".to_string(), "application/json".to_string());
            h.insert("Accept".to_string(), "application/json".to_string());
            h
        };

        let mut requests = Vec::new();
        for (ext, imps) in imp_groups {
            let mut req_copy = request.clone();
            req_copy.imp = imps;

            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };

            // Replace {{.Host}} and {{.SourceId}} in endpoint template
            let uri = self.endpoint
                .replace("{{.Host}}", &ext.host)
                .replace("{{.SourceId}}", &ext.source_id.to_string());

            requests.push(RequestData {
                method: "POST".to_string(),
                uri,
                body,
                headers: headers.clone(),
                imp_ids: get_imp_ids(&req_copy.imp),
            });
        }

        (requests, errs)
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code == 400 {
            return Err(vec![BidderError::BadInput(format!("ERR, bad input {}", response.status_code))]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!("ERR, response with status {}", response.status_code))]);
        }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::new();
        if let Some(cur) = &bid_resp.cur {
            if !cur.is_empty() { result.currency = cur.clone(); }
        }
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = get_media_type(&bid.impid, &internal.imp);
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        Ok(result)
    }
}
