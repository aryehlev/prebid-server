use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb::BidResponse;
use openrtb_ext::{BidType, ExtBidPrebidVideo};
use serde::{Deserialize, Serialize};

pub struct FreewheelsspAdapter { pub endpoint: String }
impl FreewheelsspAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

/// FreewheelSSP imp extension — zoneId can be a string or integer in JSON.
/// We store it as a string after normalising (matching Go's StringInt type).
#[derive(Debug, Default, Serialize, Deserialize)]
struct ImpExtFreewheelSSP {
    #[serde(rename = "zoneId", deserialize_with = "deser_zone_id", default)]
    zone_id: String,
}

fn deser_zone_id<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{self, Visitor};
    struct ZoneIdVisitor;
    impl<'de> Visitor<'de> for ZoneIdVisitor {
        type Value = String;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            write!(f, "a string or integer zoneId")
        }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<String, E> {
            // Validate: must be parseable as u64 (matches Go StringInt behaviour)
            v.parse::<u64>().map_err(|_| de::Error::custom(format!("zoneId '{}' is not a valid integer", v)))?;
            Ok(v.to_string())
        }
        fn visit_u64<E: de::Error>(self, v: u64) -> Result<String, E> {
            Ok(v.to_string())
        }
        fn visit_i64<E: de::Error>(self, v: i64) -> Result<String, E> {
            Ok(v.to_string())
        }
    }
    deserializer.deserialize_any(ZoneIdVisitor)
}

impl Bidder for FreewheelsspAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut req = request.clone();

        for (i, imp) in req.imp.iter_mut().enumerate() {
            let bidder_raw = imp.ext.as_ref()
                .and_then(|e| e.get("bidder"))
                .cloned();

            let bidder_val = match bidder_raw {
                Some(v) => v,
                None => {
                    return (vec![], vec![BidderError::BadInput(format!(
                        "Invalid imp.ext for impression index {}. Error Infomation: missing bidder ext", i
                    ))]);
                }
            };

            let imp_ext: ImpExtFreewheelSSP = match serde_json::from_value(bidder_val) {
                Ok(e) => e,
                Err(err) => {
                    return (vec![], vec![BidderError::BadInput(format!(
                        "Invalid imp.ext for impression index {}. Error Infomation: {}", i, err
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
            Err(e) => return (vec![], vec![BidderError::BadInput(format!(
                "Unable to transfer request to Json fomat, {}", e
            ))]),
        };

        let mut headers = HashMap::new();
        headers.insert("Componentid".to_string(), "prebid-go".to_string());

        (vec![RequestData {
            method: "POST".to_string(),
            uri: self.endpoint.clone(),
            body,
            headers,
            imp_ids: get_imp_ids(&request.imp),
        }], vec![])
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }

        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info",
                response.status_code
            ))]);
        }

        let bid_resp: BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::new();
        result.currency = bid_resp.cur.unwrap_or_default();

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let mut bid_video = ExtBidPrebidVideo::default();
                if let Some(cats) = &bid.cat {
                    if let Some(first) = cats.first() {
                        bid_video.primary_category = first.clone();
                    }
                }
                // bid.dur is not present in the openrtb Bid struct; duration stays 0
                let mut typed_bid = TypedBid::new(bid, BidType::Video);
                typed_bid.bid_video = Some(bid_video);
                result.bids.push(typed_bid);
            }
        }
        Ok(result)
    }
}
