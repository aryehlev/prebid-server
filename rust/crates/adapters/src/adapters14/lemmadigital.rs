use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;

pub struct LemmadigitalAdapter { pub endpoint: String }
impl LemmadigitalAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

#[derive(Debug, Deserialize)]
struct ImpExtLemmaDigital {
    #[serde(rename = "pid", default)]
    publisher_id: i64,
    #[serde(rename = "aid", default)]
    ad_id: i64,
}

impl Bidder for LemmadigitalAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::BadInput("Impression array should not be empty".to_string())]);
        }

        // Parse bidder ext from first impression
        let imp0 = &request.imp[0];
        let bidder_val = match imp0.ext.as_ref().and_then(|e| e.get("bidder")) {
            Some(v) => v.clone(),
            None => {
                return (vec![], vec![BidderError::BadInput(
                    format!("Invalid imp.ext for impression index 0. Error Infomation: missing bidder ext")
                )]);
            }
        };

        let imp_ext: ImpExtLemmaDigital = match serde_json::from_value(bidder_val) {
            Ok(e) => e,
            Err(err) => {
                return (vec![], vec![BidderError::BadInput(
                    format!("Invalid imp.ext.bidder for impression index 0. Error Infomation: {}", err)
                )]);
            }
        };

        // Build endpoint URL: replace {{.PublisherID}} and {{.AdUnit}}
        let url = self.endpoint
            .replace("{{.PublisherID}}", &imp_ext.publisher_id.to_string())
            .replace("{{.AdUnit}}", &imp_ext.ad_id.to_string());

        let body = match serde_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        (vec![RequestData {
            method: "POST".to_string(),
            uri: url,
            body,
            headers: HashMap::new(),
            imp_ids: get_imp_ids(&request.imp),
        }], vec![])
    }

    fn make_bids(&self, request: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        // Bid type based on first imp: video -> Video, else Banner
        let bid_type = if request.imp.first().and_then(|i| i.video.as_ref()).is_some() {
            BidType::Video
        } else {
            BidType::Banner
        };

        let mut result = BidderResponse::with_capacity(request.imp.len());
        if let Some(cur) = &bid_resp.cur {
            if !cur.is_empty() { result.currency = cur.clone(); }
        }
        if let Some(first_sb) = bid_resp.seatbid.into_iter().next() {
            for bid in first_sb.bid {
                result.bids.push(TypedBid::new(bid, bid_type.clone()));
            }
        }
        Ok(result)
    }
}
