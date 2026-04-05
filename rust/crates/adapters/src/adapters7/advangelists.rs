use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
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

            // Ensure banner has w/h set from first format entry
            if let Some(banner) = &imp_copy.banner {
                if banner.w.is_none() || banner.h.is_none() {
                    let formats = banner.format.as_deref().unwrap_or(&[]);
                    if formats.is_empty() {
                        errs.push(BidderError::BadInput("Expected at least one banner.format entry or explicit w/h".to_string()));
                        continue;
                    }
                    let mut b = banner.clone();
                    let first = formats[0].clone();
                    b.w = Some(first.w.unwrap_or(0));
                    b.h = Some(first.h.unwrap_or(0));
                    b.format = Some(formats[1..].to_vec());
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
        headers.insert("x-openrtb-version".to_string(), "2.5".to_string());

        let mut requests = Vec::new();
        for (_, (ext, imps)) in pub2imps {
            let uri = self.endpoint.replace("{{.PublisherID}}", &ext.pub_id);

            let mut req = request.clone();
            req.imp = imps.clone();

            // Set TagID from placement param on each imp
            for imp in &mut req.imp {
                if !ext.placement.is_empty() {
                    imp.tagid = Some(ext.placement.clone());
                }
            }

            // Clear site publisher and domain
            if let Some(site) = &mut req.site {
                site.publisher = None;
                site.domain = Some(String::new());
            }
            // Clear app publisher
            if let Some(app) = &mut req.app {
                app.publisher = None;
            }

            let imp_ids: Vec<String> = req.imp.iter().map(|i| i.id.clone()).collect();
            let body = match serde_json::to_vec(&req) {
                Ok(b) => b,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };
            requests.push(RequestData { method: "POST".to_string(), uri, body, headers: headers.clone(), imp_ids });
        }
        (requests, errs)
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!("Unexpected http status code: {}", response.status_code))]);
        }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(format!("Bad server response: {}", e))])?;

        if bid_resp.seatbid.len() != 1 {
            return Err(vec![BidderError::BadServerResponse(format!("Invalid SeatBids count: {}", bid_resp.seatbid.len()))]);
        }

        let seatbid = &bid_resp.seatbid[0];
        let mut result = BidderResponse::with_capacity(seatbid.bid.len());
        for bid in &seatbid.bid {
            let bid_type = internal.imp.iter()
                .find(|i| i.id == bid.impid)
                .map(|i| if i.video.is_some() { BidType::Video } else { BidType::Banner })
                .unwrap_or(BidType::Banner);
            result.bids.push(TypedBid::new(bid.clone(), bid_type));
        }
        Ok(result)
    }
}
