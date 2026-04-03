use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub struct BidmaticAdapter {
    pub endpoint: String,
}

impl BidmaticAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

#[derive(Deserialize)]
struct ExtImpBidder {
    bidder: Value,
}

#[derive(Deserialize, Clone)]
struct ExtImpBidmatic {
    #[serde(rename = "source")]
    source_id: serde_json::Number,
    #[serde(rename = "bidFloor", default)]
    bid_floor: f64,
}

#[derive(Serialize)]
struct BidmaticImpExt {
    bidmatic: ExtImpBidmaticOut,
}

#[derive(Serialize, Clone)]
struct ExtImpBidmaticOut {
    source: serde_json::Number,
    #[serde(rename = "bidFloor", skip_serializing_if = "is_zero")]
    bid_floor: f64,
}

fn is_zero(f: &f64) -> bool { *f == 0.0 }

impl Bidder for BidmaticAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        let mut errs = Vec::new();
        let mut source_to_imps: std::collections::HashMap<String, Vec<openrtb::Imp>> = std::collections::HashMap::new();

        for imp in &request.imp {
            if imp.ext.is_none() {
                errs.push(BidderError::BadInput(format!("ignoring imp id={}, extImpBidder is empty", imp.id)));
                continue;
            }

            let ext_val = imp.ext.as_ref().unwrap().clone();
            let bidder_ext: ExtImpBidder = match serde_json::from_value(ext_val) {
                Ok(v) => v,
                Err(e) => {
                    errs.push(BidderError::BadInput(format!("ignoring imp id={}, error while decoding extImpBidder, err: {}", imp.id, e)));
                    continue;
                }
            };

            let bm_ext: ExtImpBidmatic = match serde_json::from_value(bidder_ext.bidder) {
                Ok(v) => v,
                Err(e) => {
                    errs.push(BidderError::BadInput(format!("ignoring imp id={}, error while decoding impExt, err: {}", imp.id, e)));
                    continue;
                }
            };

            let new_ext = BidmaticImpExt {
                bidmatic: ExtImpBidmaticOut {
                    source: bm_ext.source_id.clone(),
                    bid_floor: bm_ext.bid_floor,
                },
            };

            let new_ext_val = match serde_json::to_value(&new_ext) {
                Ok(v) => v,
                Err(e) => {
                    errs.push(BidderError::BadInput(format!("ignoring imp id={}, error while marshaling impExt, err: {}", imp.id, e)));
                    continue;
                }
            };

            let source_key = bm_ext.source_id.to_string();

            let mut imp_copy = imp.clone();
            imp_copy.ext = Some(new_ext_val);
            if bm_ext.bid_floor > 0.0 {
                imp_copy.bidfloor = Some(bm_ext.bid_floor);
            }

            source_to_imps.entry(source_key).or_default().push(imp_copy);
        }

        if source_to_imps.is_empty() {
            return (vec![], errs);
        }

        let mut results = Vec::new();
        for (source_id, imps) in source_to_imps {
            let mut req_copy = request.clone();
            req_copy.imp = imps;

            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(BidderError::BadInput(format!("error while encoding bidRequest, err: {}", e)));
                    return (vec![], errs);
                }
            };

            results.push(RequestData {
                method: "POST".to_string(),
                uri: format!("{}?source={}", self.endpoint, source_id),
                body,
                headers: headers.clone(),
                imp_ids: get_imp_ids(&req_copy.imp),
            });
        }

        (results, errs)
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) {
            return Err(vec![e]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(format!(
                "error while decoding response, err: {}", e
            ))])?;

        let mut result = BidderResponse::new();
        let mut errs = Vec::new();

        for sb in bid_resp.seatbid {
            for mut bid in sb.bid {
                let mut found = false;
                let mut media_type = BidType::Banner;

                for imp in &internal.imp {
                    if imp.id == bid.impid {
                        found = true;
                        if imp.video.is_some() {
                            media_type = BidType::Video;
                            bid.mtype = Some(2);
                        } else if imp.banner.is_some() {
                            media_type = BidType::Banner;
                            bid.mtype = Some(1);
                        } else if imp.audio.is_some() {
                            media_type = BidType::Audio;
                            bid.mtype = Some(3);
                        } else if imp.native.is_some() {
                            media_type = BidType::Native;
                            bid.mtype = Some(4);
                        } else {
                            bid.mtype = Some(1);
                        }
                        break;
                    }
                }

                if !found {
                    errs.push(BidderError::BadServerResponse(format!(
                        "ignoring bid id={}, request doesn't contain any impression with id={}", bid.id, bid.impid
                    )));
                    continue;
                }

                result.bids.push(TypedBid::new(bid, media_type));
            }
        }

        Ok(result)
    }
}
