use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::{BidType, ExtBidPrebidVideo};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub struct FwsspAdapter {
    pub endpoint: String,
}

impl FwsspAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

/// FWSSP imp extension
#[derive(Debug, Default, Deserialize, Serialize)]
struct ImpExtFwssp {
    #[serde(rename = "publisherId", skip_serializing_if = "String::is_empty", default)]
    publisher_id: String,
    #[serde(rename = "adSlot", skip_serializing_if = "String::is_empty", default)]
    ad_slot: String,
    #[serde(rename = "adNetwork", skip_serializing_if = "String::is_empty", default)]
    ad_network: String,
    #[serde(rename = "adUnitId", skip_serializing_if = "String::is_empty", default)]
    ad_unit_id: String,
}

impl Bidder for FwsspAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut req = request.clone();

        for (i, imp) in req.imp.iter_mut().enumerate() {
            let bidder_ext = imp.ext.as_ref()
                .and_then(|e| e.get("bidder"))
                .cloned();

            let imp_ext: ImpExtFwssp = match bidder_ext {
                Some(b) => match serde_json::from_value(b) {
                    Ok(e) => e,
                    Err(err) => {
                        return (vec![], vec![BidderError::BadInput(format!(
                            "Invalid imp.ext for impression index {}. Error Infomation: {}", i, err
                        ))]);
                    }
                },
                None => {
                    return (vec![], vec![BidderError::BadInput(format!(
                        "Invalid imp.ext for impression index {}. Error Infomation: missing bidder ext", i
                    ))]);
                }
            };

            imp.ext = match serde_json::to_value(&imp_ext) {
                Ok(v) => Some(v),
                Err(err) => {
                    return (vec![], vec![BidderError::BadInput(format!(
                        "Unable to transfer requestImpExt to Json fomat, {}", err
                    ))]);
                }
            };
        }

        let body = match serde_json::to_vec(&req) {
            Ok(b) => b,
            Err(e) => {
                return (vec![], vec![BidderError::BadInput(format!(
                    "Unable to transfer request to Json fomat, {}", e
                ))]);
            }
        };

        let mut headers = HashMap::new();
        headers.insert("Componentid".to_string(), "prebid-go".to_string());

        (
            vec![RequestData {
                method: "POST".to_string(),
                uri: self.endpoint.clone(),
                body,
                headers,
                imp_ids: get_imp_ids(&req.imp),
            }],
            vec![],
        )
    }

    fn make_bids(
        &self,
        _internal: &openrtb::BidRequest,
        _external: &RequestData,
        response: &ResponseData,
    ) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 || (response.status_code == 200 && response.body.is_empty()) {
            return Ok(BidderResponse::new());
        }

        if response.status_code == 400 {
            return Err(vec![BidderError::BadInput(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info",
                response.status_code
            ))]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info",
                response.status_code
            ))]);
        }

        let bid_response: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(_internal.imp.len());
        if let Some(cur) = &bid_response.cur {
            if !cur.is_empty() {
                result.currency = cur.clone();
            }
        }

        for seat_bid in bid_response.seatbid {
            for bid in seat_bid.bid {
                let mut bid_video = ExtBidPrebidVideo::default();
                if let Some(cat) = &bid.cat {
                    if let Some(first) = cat.first() {
                        bid_video.primary_category = first.clone();
                    }
                }
                // Note: bid.dur doesn't exist in our Bid struct; skip it

                let mut typed_bid = TypedBid::new(bid, BidType::Video);
                typed_bid.bid_video = Some(bid_video);
                result.bids.push(typed_bid);
            }
        }

        Ok(result)
    }
}
