use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub struct AdtargetAdapter { pub endpoint: String }
impl AdtargetAdapter { pub fn new(endpoint: String) -> Self { Self { endpoint } } }

#[derive(Deserialize)]
struct ExtImpBidder { bidder: Value }

#[derive(Deserialize)]
struct ExtImpAdtarget {
    aid: serde_json::Number,
    #[serde(rename = "placementId", default)]
    placement_id: i32,
    #[serde(rename = "siteId", default)]
    site_id: i32,
    #[serde(rename = "bidFloor", default)]
    bid_floor: f64,
}

#[derive(Serialize)]
struct AdtargetImpExt {
    adtarget: AdtargetImpExtInner,
}

#[derive(Serialize, Deserialize)]
struct AdtargetImpExtInner {
    aid: serde_json::Number,
    #[serde(rename = "placementId", default, skip_serializing_if = "is_zero_i32")]
    placement_id: i32,
    #[serde(rename = "siteId", default, skip_serializing_if = "is_zero_i32")]
    site_id: i32,
    #[serde(rename = "bidFloor", default, skip_serializing_if = "is_zero_f64")]
    bid_floor: f64,
}

fn is_zero_i32(v: &i32) -> bool { *v == 0 }
fn is_zero_f64(v: &f64) -> bool { *v == 0.0 }

fn validate_and_set_ext(imp: &mut openrtb::Imp) -> Result<i64, BidderError> {
    if imp.banner.is_none() && imp.video.is_none() {
        return Err(BidderError::BadInput(format!("ignoring imp id={}, Adtarget supports only Video and Banner", imp.id)));
    }
    let ext = imp.ext.as_ref().ok_or_else(|| BidderError::BadInput(format!("ignoring imp id={}, extImpBidder is empty", imp.id)))?;
    let bidder_ext: ExtImpBidder = serde_json::from_value(ext.clone())
        .map_err(|e| BidderError::BadInput(format!("ignoring imp id={}, error while decoding extImpBidder, err: {}", imp.id, e)))?;
    let imp_ext: ExtImpAdtarget = serde_json::from_value(bidder_ext.bidder)
        .map_err(|e| BidderError::BadInput(format!("ignoring imp id={}, error while decoding impExt, err: {}", imp.id, e)))?;

    let aid = imp_ext.aid.as_i64()
        .ok_or_else(|| BidderError::BadInput(format!("ignoring imp id={}, aid parsing err", imp.id)))?;

    if imp_ext.bid_floor > 0.0 {
        imp.bidfloor = Some(imp_ext.bid_floor);
    }

    let new_ext = AdtargetImpExt {
        adtarget: AdtargetImpExtInner {
            aid: imp_ext.aid,
            placement_id: imp_ext.placement_id,
            site_id: imp_ext.site_id,
            bid_floor: imp_ext.bid_floor,
        },
    };
    imp.ext = Some(serde_json::to_value(&new_ext)
        .map_err(|e| BidderError::BadInput(format!("ignoring imp id={}, error encoding impExt, err: {}", imp.id, e)))?);
    Ok(aid)
}

impl Bidder for AdtargetAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut source2imps: HashMap<i64, Vec<openrtb::Imp>> = HashMap::new();

        let mut imps = request.imp.clone();
        for imp in &mut imps {
            match validate_and_set_ext(imp) {
                Ok(aid) => { source2imps.entry(aid).or_default().push(imp.clone()); }
                Err(e) => errs.push(e),
            }
        }
        if source2imps.is_empty() { return (vec![], errs); }

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        let mut requests = Vec::new();
        for (aid, source_imps) in source2imps {
            let mut req = request.clone();
            req.imp = source_imps;
            let body = match serde_json::to_vec(&req) {
                Ok(b) => b,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };
            let imp_ids = get_imp_ids(&req.imp);
            requests.push(RequestData {
                method: "POST".to_string(),
                uri: format!("{}?aid={}", self.endpoint, aid),
                body, headers: headers.clone(), imp_ids,
            });
        }
        (requests, errs)
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code == 400 {
            return Err(vec![BidderError::BadInput(format!("Unexpected status code: {}. Run with request.debug = 1 for more info", response.status_code))]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!("Unexpected status code: {}.", response.status_code))]);
        }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(format!("error while decoding response, err: {}", e))])?;
        let mut result = BidderResponse::new();
        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let mut bid_type = BidType::Banner;
                let mut found = false;
                for imp in &internal.imp {
                    if imp.id == bid.impid {
                        found = true;
                        if imp.video.is_some() { bid_type = BidType::Video; }
                        break;
                    }
                }
                if !found {
                    errs.push(BidderError::BadServerResponse(format!("ignoring bid id={}, request doesn't contain any impression with id={}", bid.id, bid.impid)));
                    continue;
                }
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        Ok(result)
    }
}
