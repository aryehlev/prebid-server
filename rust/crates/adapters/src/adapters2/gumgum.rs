use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;

pub struct GumgumAdapter { pub endpoint: String }
impl GumgumAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

#[derive(Debug, Default, Deserialize)]
struct ExtImpGumGum {
    #[serde(default)]
    zone: String,
    #[serde(rename = "pubId", default)]
    pub_id: f64,
}

fn get_media_type_for_imp_id(imp_id: &str, imps: &[openrtb::Imp]) -> BidType {
    for imp in imps {
        if imp.id == imp_id && imp.banner.is_some() {
            return BidType::Banner;
        }
    }
    BidType::Video
}

impl Bidder for GumgumAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut valid_imps: Vec<openrtb::Imp> = Vec::new();
        let mut site_id: Option<String> = None;
        let mut publisher_id: Option<String> = None;

        for imp in &request.imp {
            let bidder_val = match imp.ext.as_ref().and_then(|e| e.get("bidder")).cloned() {
                Some(v) => v,
                None => {
                    errs.push(BidderError::BadInput(format!("imp {} missing bidder ext", imp.id)));
                    continue;
                }
            };
            let gumgum_ext: ExtImpGumGum = match serde_json::from_value(bidder_val) {
                Ok(e) => e,
                Err(e) => {
                    errs.push(BidderError::BadInput(e.to_string()));
                    continue;
                }
            };

            if !gumgum_ext.zone.is_empty() {
                site_id = Some(gumgum_ext.zone.clone());
            }
            if gumgum_ext.pub_id != 0.0 {
                // Format float without trailing zeros
                let s = if gumgum_ext.pub_id.fract() == 0.0 {
                    format!("{}", gumgum_ext.pub_id as i64)
                } else {
                    gumgum_ext.pub_id.to_string()
                };
                publisher_id = Some(s);
            }

            valid_imps.push(imp.clone());
        }

        if valid_imps.is_empty() {
            return (vec![], errs);
        }

        let mut req = request.clone();
        req.imp = valid_imps;

        // Update site with zone and publisher
        if req.site.is_some() {
            let site = req.site.as_mut().unwrap();
            if let Some(sid) = site_id {
                site.id = Some(sid);
            }
            if let Some(pid) = publisher_id {
                let pub_obj = site.publisher.get_or_insert_with(Default::default);
                pub_obj.id = Some(pid);
            }
        }

        let body = match serde_json::to_vec(&req) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        (vec![RequestData { method: "POST".to_string(), uri: self.endpoint.clone(), body, headers, imp_ids: get_imp_ids(&req.imp) }], errs)
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(5);
        if let Some(cur) = bid_resp.cur {
            if !cur.is_empty() {
                result.currency = cur;
            }
        }
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = get_media_type_for_imp_id(&bid.impid, &internal.imp);
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        Ok(result)
    }
}
