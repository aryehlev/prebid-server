use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;
use serde_json::Value;

pub struct AjaAdapter {
    pub endpoint: String,
}

impl AjaAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

#[derive(Deserialize)]
struct ExtImpBidder { bidder: Value }

#[derive(Deserialize)]
struct ExtImpAJA {
    #[serde(rename = "asi", default)]
    ad_spot_id: String,
}

impl Bidder for AjaAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        // Group imps by tagid (ad_spot_id), then create one request per group
        let mut tag_ids: Vec<String> = Vec::new();
        let mut imps_by_tag: std::collections::HashMap<String, Vec<openrtb::Imp>> = std::collections::HashMap::new();
        let mut errs = Vec::new();

        for imp in &request.imp {
            let ext_val = match &imp.ext {
                Some(v) => v.clone(),
                None => { errs.push(BidderError::BadInput(format!("Failed to unmarshal ext impID: {}", imp.id))); continue; }
            };

            let bidder_ext: ExtImpBidder = match serde_json::from_value(ext_val) {
                Ok(v) => v,
                Err(e) => { errs.push(BidderError::BadInput(format!("Failed to unmarshal ext impID: {} err: {}", imp.id, e))); continue; }
            };

            let aja_ext: ExtImpAJA = match serde_json::from_value(bidder_ext.bidder) {
                Ok(v) => v,
                Err(e) => { errs.push(BidderError::BadInput(format!("Failed to unmarshal ext.bidder impID: {} err: {}", imp.id, e))); continue; }
            };

            let mut new_imp = imp.clone();
            new_imp.tagid = Some(aja_ext.ad_spot_id.clone());
            new_imp.ext = None;

            let tag_id = aja_ext.ad_spot_id;
            if !imps_by_tag.contains_key(&tag_id) {
                tag_ids.push(tag_id.clone());
            }
            imps_by_tag.entry(tag_id).or_default().push(new_imp);
        }

        let mut requests = Vec::new();
        for tag_id in &tag_ids {
            let tag_imps = imps_by_tag.remove(tag_id).unwrap_or_default();
            let mut req = request.clone();
            req.imp = tag_imps;

            let body = match serde_json::to_vec(&req) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(BidderError::BadInput(format!("Failed to unmarshal bidrequest ID: {} err: {}", request.id, e)));
                    continue;
                }
            };

            let imp_ids = get_imp_ids(&req.imp);
            requests.push(RequestData {
                method: "POST".to_string(),
                uri: self.endpoint.clone(),
                body,
                headers: HashMap::new(),
                imp_ids,
            });
        }

        (requests, errs)
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code == 400 {
            return Err(vec![BidderError::BadInput(format!("Unexpected status code: {}", response.status_code))]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!("Unexpected status code: {}", response.status_code))]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(format!("Failed to unmarshal bid response: {}", e))])?;

        let mut result = BidderResponse::with_capacity(internal.imp.len());
        let mut errs = Vec::new();

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let mut found = false;
                for imp in &internal.imp {
                    if imp.id == bid.impid {
                        found = true;
                        let bid_type = if imp.banner.is_some() {
                            BidType::Banner
                        } else if imp.video.is_some() {
                            BidType::Video
                        } else {
                            errs.push(BidderError::BadServerResponse(
                                format!("Response received for unexpected type of bid bidID: {}", bid.id)
                            ));
                            break;
                        };
                        result.bids.push(TypedBid::new(bid.clone(), bid_type));
                        break;
                    }
                }
                let _ = found;
            }
        }

        if let Some(cur) = &bid_resp.cur {
            if !cur.is_empty() { result.currency = cur.clone(); }
        }

        Ok(result)
    }
}
