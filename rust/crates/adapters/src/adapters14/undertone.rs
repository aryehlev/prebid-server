use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::{Deserialize, Serialize};

pub struct UndertoneAdapter { pub endpoint: String }
impl UndertoneAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

const ADAPTER_ID: i64 = 4;
const ADAPTER_VERSION: &str = "1.0.0";

#[derive(Debug, Deserialize)]
struct ExtImpUndertone {
    #[serde(rename = "publisherId", default)]
    publisher_id: i64,
    #[serde(rename = "placementId", default)]
    placement_id: i64,
}

#[derive(Debug, Deserialize)]
struct ImpExt {
    bidder: Option<ExtImpUndertone>,
    #[serde(default)]
    gpid: String,
}

#[derive(Debug, Serialize)]
struct UndertoneParams {
    id: i64,
    version: String,
}

impl Bidder for UndertoneAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut valid_imps = Vec::new();
        let mut publisher_id: i64 = 0;

        for imp in &request.imp {
            let ext_val = match &imp.ext {
                Some(e) => e.clone(),
                None => {
                    errs.push(BidderError::BadInput(format!("Invalid impid={}: missing ext", imp.id)));
                    continue;
                }
            };

            let imp_ext: ImpExt = match serde_json::from_value(ext_val) {
                Ok(e) => e,
                Err(err) => {
                    errs.push(BidderError::BadInput(format!("Invalid impid={}: {}", imp.id, err)));
                    continue;
                }
            };

            let bidder_ext = match imp_ext.bidder {
                Some(b) => b,
                None => {
                    errs.push(BidderError::BadInput(format!("Invalid impid={}: missing bidder ext", imp.id)));
                    continue;
                }
            };

            if publisher_id == 0 {
                publisher_id = bidder_ext.publisher_id;
            }

            let mut imp_copy = imp.clone();
            imp_copy.tagid = Some(bidder_ext.placement_id.to_string());

            if !imp_ext.gpid.is_empty() {
                // Set ext to just gpid (no bidder)
                #[derive(Serialize)]
                struct GpidExt { gpid: String }
                imp_copy.ext = Some(serde_json::to_value(GpidExt { gpid: imp_ext.gpid }).unwrap_or(serde_json::Value::Null));
            } else {
                imp_copy.ext = None;
            }

            valid_imps.push(imp_copy);
        }

        if valid_imps.is_empty() {
            return (vec![], errs);
        }

        let mut req_copy = request.clone();
        req_copy.imp = valid_imps;

        // Set site/app publisher ID
        let pub_id_str = publisher_id.to_string();
        if let Some(site) = &req_copy.site {
            let mut site_copy = site.clone();
            let mut publisher = site_copy.publisher.clone().unwrap_or_default();
            publisher.id = Some(pub_id_str.clone());
            site_copy.publisher = Some(publisher);
            req_copy.site = Some(site_copy);
        } else if let Some(app) = &req_copy.app {
            let mut app_copy = app.clone();
            let mut publisher = app_copy.publisher.clone().unwrap_or_default();
            publisher.id = Some(pub_id_str.clone());
            app_copy.publisher = Some(publisher);
            req_copy.app = Some(app_copy);
        }

        // Set request ext
        let params = UndertoneParams { id: ADAPTER_ID, version: ADAPTER_VERSION.to_string() };
        match serde_json::to_value(&params) {
            Ok(v) => req_copy.ext = Some(v),
            Err(e) => errs.push(BidderError::BadInput(e.to_string())),
        }

        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => return (vec![], { errs.push(BidderError::BadInput(e.to_string())); errs }),
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());

        (vec![RequestData {
            method: "POST".to_string(),
            uri: self.endpoint.clone(),
            body,
            headers,
            imp_ids: get_imp_ids(&req_copy.imp),
        }], errs)
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code == 400 {
            return Err(vec![BidderError::BadInput(
                "Unexpected status code: 400. Bad request from publisher. Run with request.debug = 1 for more info.".to_string()
            )]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(
                format!("Unexpected status code: {}. Run with request.debug = 1 for more info.", response.status_code)
            )]);
        }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        // Build imp -> bid type map
        let mut imp_bid_type: HashMap<String, BidType> = HashMap::new();
        for imp in &internal.imp {
            if imp.banner.is_some() {
                imp_bid_type.insert(imp.id.clone(), BidType::Banner);
            } else if imp.video.is_some() {
                imp_bid_type.insert(imp.id.clone(), BidType::Video);
            }
        }

        let mut result = BidderResponse::with_capacity(internal.imp.len());
        result.currency = bid_resp.cur.clone().unwrap_or_default();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                if let Some(bid_type) = imp_bid_type.get(&bid.impid).cloned() {
                    result.bids.push(TypedBid::new(bid, bid_type));
                }
            }
        }
        Ok(result)
    }
}
