use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;

pub struct SovrnXspAdapter { pub endpoint: String }
impl SovrnXspAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

// creative_type values from XSP
const CREATIVE_TYPE_BANNER: i64 = 0;
const CREATIVE_TYPE_VIDEO: i64 = 1;
const CREATIVE_TYPE_NATIVE: i64 = 2;

#[derive(Debug, Deserialize, Default)]
struct ExtImpSovrnXsp {
    #[serde(rename = "pub_id", default)]
    pub_id: String,
    #[serde(rename = "med_id", default)]
    med_id: String,
    #[serde(rename = "zone_id", default)]
    zone_id: String,
}

impl Bidder for SovrnXspAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut imps = Vec::new();
        let mut req_copy = request.clone();

        // Ensure app exists
        if req_copy.app.is_none() {
            req_copy.app = Some(openrtb::App::default());
        }

        for (idx, imp) in request.imp.iter().enumerate() {
            if imp.banner.is_none() && imp.video.is_none() && imp.native.is_none() {
                continue;
            }

            let bidder_val = match imp.ext.as_ref().and_then(|e| e.get("bidder")) {
                Some(v) => v.clone(),
                None => {
                    errs.push(BidderError::BadInput(format!("imp #{}: ext.bidder not provided", idx)));
                    continue;
                }
            };

            let xsp_ext: ExtImpSovrnXsp = match serde_json::from_value(bidder_val) {
                Ok(e) => e,
                Err(e) => {
                    errs.push(BidderError::BadInput(format!("imp #{}: {}", idx, e)));
                    continue;
                }
            };

            if let Some(app) = req_copy.app.as_mut() {
                if app.publisher.is_none() {
                    app.publisher = Some(openrtb::Publisher::default());
                }
                if let Some(pub_) = app.publisher.as_mut() {
                    pub_.id = Some(xsp_ext.pub_id.clone());
                }
                if !xsp_ext.med_id.is_empty() {
                    app.id = Some(xsp_ext.med_id.clone());
                }
            }

            let mut imp_copy = imp.clone();
            if !xsp_ext.zone_id.is_empty() {
                imp_copy.tagid = Some(xsp_ext.zone_id.clone());
            }
            imps.push(imp_copy);
        }

        if imps.is_empty() {
            errs.push(BidderError::BadInput("no matching impression with ad format".to_string()));
            return (vec![], errs);
        }

        req_copy.imp = imps;

        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => {
                errs.push(BidderError::BadInput(e.to_string()));
                return (vec![], errs);
            }
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
        headers.insert("x-openrtb-version".to_string(), "2.5".to_string());

        (vec![RequestData {
            method: "POST".to_string(),
            uri: self.endpoint.clone(),
            body,
            headers,
            imp_ids: get_imp_ids(&req_copy.imp),
        }], errs)
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(5);
        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for mut bid in sb.bid {
                let creative_type = bid.ext.as_ref()
                    .and_then(|e| e.get("creative_type"))
                    .and_then(|v| v.as_i64())
                    .unwrap_or(-1);

                let (bid_type, mtype) = match creative_type {
                    CREATIVE_TYPE_BANNER => (BidType::Banner, 1i32),
                    CREATIVE_TYPE_VIDEO => (BidType::Video, 2i32),
                    CREATIVE_TYPE_NATIVE => (BidType::Native, 4i32),
                    _ => {
                        errs.push(BidderError::BadServerResponse(format!(
                            "Unsupported creative type: {}", creative_type
                        )));
                        continue;
                    }
                };

                if bid.mtype.is_none() || bid.mtype == Some(0) {
                    bid.mtype = Some(mtype);
                }

                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        if result.bids.is_empty() {
            return Err(errs);
        }
        Ok(result)
    }
}
