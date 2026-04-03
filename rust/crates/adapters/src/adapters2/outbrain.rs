use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct OutbrainAdapter {
    pub endpoint: String,
}

impl OutbrainAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

fn get_media_type_for_imp(imp_id: &str, imps: &[openrtb::Imp]) -> Result<BidType, BidderError> {
    for imp in imps {
        if imp.id == imp_id {
            if imp.native.is_some() {
                return Ok(BidType::Native);
            } else if imp.banner.is_some() {
                return Ok(BidType::Banner);
            } else if imp.video.is_some() {
                return Ok(BidType::Video);
            }
        }
    }
    Err(BidderError::BadInput(format!(
        "Failed to find native/banner/video impression \"{}\"",
        imp_id
    )))
}

impl Bidder for OutbrainAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut req_copy = request.clone();
        let mut outbrain_publisher_id = String::new();
        let mut outbrain_publisher_name = String::new();
        let mut outbrain_publisher_domain = String::new();
        let mut outbrain_bcat: Option<Vec<String>> = None;
        let mut outbrain_badv: Option<Vec<String>> = None;

        for (i, imp) in request.imp.iter().enumerate() {
            let ext = match imp.ext.as_ref() {
                Some(e) => e,
                None => continue,
            };
            let bidder_ext = match ext.get("bidder") {
                Some(b) => b,
                None => continue,
            };

            let tag_id = bidder_ext.get("tagId")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            if !tag_id.is_empty() {
                req_copy.imp[i].tagid = Some(tag_id);
            }

            // Publisher info from outbrain ext
            if let Some(pub_obj) = bidder_ext.get("publisher") {
                outbrain_publisher_id = pub_obj.get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                outbrain_publisher_name = pub_obj.get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                outbrain_publisher_domain = pub_obj.get("domain")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
            }

            if let Some(bcat) = bidder_ext.get("bcat") {
                if let Some(arr) = bcat.as_array() {
                    outbrain_bcat = Some(arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect());
                }
            }
            if let Some(badv) = bidder_ext.get("badv") {
                if let Some(arr) = badv.as_array() {
                    outbrain_badv = Some(arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect());
                }
            }
        }

        // Set publisher on site or app
        let publisher = openrtb::Publisher {
            id: Some(outbrain_publisher_id),
            name: Some(outbrain_publisher_name),
            domain: Some(outbrain_publisher_domain),
            ..Default::default()
        };

        if let Some(site) = req_copy.site.as_mut() {
            site.publisher = Some(publisher);
        } else if let Some(app) = req_copy.app.as_mut() {
            app.publisher = Some(publisher);
        }

        if let Some(bcat) = outbrain_bcat {
            req_copy.bcat = Some(bcat);
        }
        if let Some(badv) = outbrain_badv {
            req_copy.badv = Some(badv);
        }

        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let imp_ids = get_imp_ids(&req_copy.imp);
        (
            vec![RequestData {
                method: "POST".to_string(),
                uri: self.endpoint.clone(),
                body,
                headers: HashMap::new(),
                imp_ids,
            }],
            errs,
        )
    }

    fn make_bids(
        &self,
        internal: &openrtb::BidRequest,
        _external: &RequestData,
        response: &ResponseData,
    ) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 {
            return Ok(BidderResponse::new());
        }
        if let Err(e) = crate::check_response_status(response.status_code) {
            return Err(vec![e]);
        }

        let bid_response: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(internal.imp.len());
        if let Some(cur) = bid_response.cur.as_deref() {
            if !cur.is_empty() {
                result.currency = cur.to_string();
            }
        }

        let mut errs = Vec::new();
        for sb in bid_response.seatbid {
            for bid in sb.bid {
                match get_media_type_for_imp(&bid.impid, &internal.imp) {
                    Ok(bid_type) => result.bids.push(TypedBid::new(bid, bid_type)),
                    Err(e) => errs.push(e),
                }
            }
        }

        Ok(result)
    }
}
