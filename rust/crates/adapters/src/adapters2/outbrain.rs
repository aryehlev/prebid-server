use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;

pub struct OutbrainAdapter { pub endpoint: String }
impl OutbrainAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

#[derive(Debug, Default, Deserialize)]
struct ExtImpOutbrainPublisher {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    domain: String,
}

#[derive(Debug, Default, Deserialize)]
struct ExtImpOutbrain {
    #[serde(rename = "tagId", default)]
    tag_id: String,
    #[serde(default)]
    publisher: ExtImpOutbrainPublisher,
    #[serde(rename = "bcat", default)]
    bcat: Option<Vec<String>>,
    #[serde(rename = "badv", default)]
    badv: Option<Vec<String>>,
}

fn get_media_type_for_imp(imp_id: &str, imps: &[openrtb::Imp]) -> BidType {
    for imp in imps {
        if imp.id == imp_id {
            if imp.native.is_some() {
                return BidType::Native;
            } else if imp.banner.is_some() {
                return BidType::Banner;
            } else if imp.video.is_some() {
                return BidType::Video;
            }
        }
    }
    BidType::Banner
}

impl Bidder for OutbrainAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut req = request.clone();
        let mut errs = Vec::new();
        let mut outbrain_ext = ExtImpOutbrain::default();

        for imp in req.imp.iter_mut() {
            let bidder_ext = match imp.ext.as_ref()
                .and_then(|e| e.get("bidder"))
                .cloned()
            {
                Some(e) => e,
                None => continue,
            };
            let ext: ExtImpOutbrain = match serde_json::from_value(bidder_ext) {
                Ok(e) => e,
                Err(e) => {
                    errs.push(BidderError::BadInput(e.to_string()));
                    continue;
                }
            };
            if !ext.tag_id.is_empty() {
                imp.tagid = Some(ext.tag_id.clone());
            }
            outbrain_ext = ext;
        }

        // Set publisher on site or app
        let publisher = openrtb::Publisher {
            id: if outbrain_ext.publisher.id.is_empty() { None } else { Some(outbrain_ext.publisher.id.clone()) },
            name: if outbrain_ext.publisher.name.is_empty() { None } else { Some(outbrain_ext.publisher.name.clone()) },
            domain: if outbrain_ext.publisher.domain.is_empty() { None } else { Some(outbrain_ext.publisher.domain.clone()) },
            ..Default::default()
        };

        if let Some(site) = req.site.as_mut() {
            site.publisher = Some(publisher);
        } else if let Some(app) = req.app.as_mut() {
            app.publisher = Some(publisher);
        }

        if let Some(bcat) = outbrain_ext.bcat {
            req.bcat = Some(bcat);
        }
        if let Some(badv) = outbrain_ext.badv {
            req.badv = Some(badv);
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
            result.currency = cur;
        }
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = get_media_type_for_imp(&bid.impid, &internal.imp);
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        Ok(result)
    }
}
