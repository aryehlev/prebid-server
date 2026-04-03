use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use openrtb_ext::{BidType, ExtBidPrebidVideo};
use serde::{Deserialize, Serialize};

const MAX_IMPS_PER_REQ: usize = 10;

pub struct MsftAdapter { pub endpoint: String }
impl MsftAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

#[derive(Debug, Deserialize, Default)]
struct ImpExtBidderMsft {
    #[serde(rename = "placementId", default)]
    placement_id: i64,
    #[serde(rename = "invCode", default)]
    inv_code: String,
    #[serde(rename = "member", default)]
    member: i64,
    #[serde(rename = "trafficSourceCode", default)]
    traffic_source_code: String,
    #[serde(rename = "keywords", default)]
    keywords: String,
    #[serde(rename = "pubClick", default)]
    pub_click: String,
    #[serde(rename = "extInvCode", default)]
    ext_inv_code: String,
    #[serde(rename = "extImpId", default)]
    ext_imp_id: String,
    #[serde(rename = "usePaymentRule", default)]
    use_payment_rule: Option<bool>,
    #[serde(rename = "allowSmallerSizes", default)]
    allow_smaller_sizes: Option<bool>,
}

#[derive(Debug, Deserialize, Default)]
struct ImpExtIncoming {
    #[serde(default)]
    bidder: ImpExtBidderMsft,
    #[serde(default)]
    gpid: String,
}

#[derive(Debug, Serialize, Default)]
struct ImpExtOutgoingAppnexus {
    #[serde(rename = "placement_id", skip_serializing_if = "is_zero_i64")]
    placement_id: i64,
    #[serde(rename = "allow_smaller_sizes", skip_serializing_if = "Option::is_none")]
    allow_smaller_sizes: Option<bool>,
    #[serde(rename = "use_pmt_rule", skip_serializing_if = "Option::is_none")]
    use_pmt_rule: Option<bool>,
    #[serde(rename = "keywords", skip_serializing_if = "String::is_empty")]
    keywords: String,
    #[serde(rename = "traffic_source_code", skip_serializing_if = "String::is_empty")]
    traffic_source_code: String,
    #[serde(rename = "pub_click", skip_serializing_if = "String::is_empty")]
    pub_click: String,
    #[serde(rename = "ext_inv_code", skip_serializing_if = "String::is_empty")]
    ext_inv_code: String,
    #[serde(rename = "ext_imp_id", skip_serializing_if = "String::is_empty")]
    ext_imp_id: String,
}

#[derive(Debug, Serialize, Default)]
struct ImpExtOutgoing {
    appnexus: ImpExtOutgoingAppnexus,
    #[serde(skip_serializing_if = "String::is_empty")]
    gpid: String,
}

fn is_zero_i64(v: &i64) -> bool { *v == 0 }

#[derive(Debug, Deserialize, Default)]
struct BidExtAppnexus {
    #[serde(rename = "bid_ad_type", default)]
    bid_type: i32,
    #[serde(rename = "brand_category_id", default)]
    brand_category: i32,
    #[serde(rename = "deal_priority", default)]
    deal_priority: i32,
    #[serde(default)]
    creative_info: BidExtCreativeInfo,
}

#[derive(Debug, Deserialize, Default)]
struct BidExtCreativeInfo {
    #[serde(default)]
    video: BidExtVideo,
}

#[derive(Debug, Deserialize, Default)]
struct BidExtVideo {
    #[serde(default)]
    duration: i32,
}

#[derive(Debug, Deserialize, Default)]
struct BidExt {
    #[serde(default)]
    appnexus: BidExtAppnexus,
}

fn get_media_type_for_bid(bid_type: i32) -> Result<BidType, BidderError> {
    match bid_type {
        0 => Ok(BidType::Banner),
        1 => Ok(BidType::Video),
        3 => Ok(BidType::Native),
        _ => Err(BidderError::BadServerResponse(format!("unrecognized bid_ad_type in response: {}", bid_type))),
    }
}

impl Bidder for MsftAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _info: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
        headers.insert("x-openrtb-version".to_string(), "2.6".to_string());

        let mut valid_imps = Vec::new();
        let mut unique_member_id: i64 = 0;

        for imp in &request.imp {
            let ext_val = match &imp.ext {
                Some(v) => v.clone(),
                None => { errs.push(BidderError::BadInput(format!("failed to parse ext for impression id '{}'", imp.id))); continue; }
            };
            let imp_ext: ImpExtIncoming = match serde_json::from_value(ext_val) {
                Ok(v) => v,
                Err(_) => { errs.push(BidderError::BadInput(format!("failed to parse ext for impression id '{}'", imp.id))); continue; }
            };

            let member_id = imp_ext.bidder.member;
            if member_id != 0 {
                if unique_member_id == 0 {
                    unique_member_id = member_id;
                } else if unique_member_id != member_id {
                    errs.push(BidderError::BadInput("member id mismatch: all impressions must use the same member id".to_string()));
                    return (vec![], errs);
                }
            }

            let mut imp_copy = imp.clone();
            // Apply inv_code as tagid
            if !imp_ext.bidder.inv_code.is_empty() {
                imp_copy.tagid = Some(imp_ext.bidder.inv_code.clone());
            }
            // Assign banner size from first format if missing
            if let Some(banner) = imp_copy.banner.as_mut() {
                if banner.w.is_none() && banner.h.is_none() {
                    if let Some(fmt) = banner.format.as_deref().and_then(|f| f.first()) {
                        banner.w = Some(fmt.w.unwrap_or(0));
                        banner.h = Some(fmt.h.unwrap_or(0));
                    }
                }
            }
            // Build outgoing ext
            let out_ext = ImpExtOutgoing {
                appnexus: ImpExtOutgoingAppnexus {
                    placement_id: imp_ext.bidder.placement_id,
                    allow_smaller_sizes: imp_ext.bidder.allow_smaller_sizes,
                    use_pmt_rule: imp_ext.bidder.use_payment_rule,
                    keywords: imp_ext.bidder.keywords,
                    traffic_source_code: imp_ext.bidder.traffic_source_code,
                    pub_click: imp_ext.bidder.pub_click,
                    ext_inv_code: imp_ext.bidder.ext_inv_code,
                    ext_imp_id: imp_ext.bidder.ext_imp_id,
                },
                gpid: imp_ext.gpid,
            };
            imp_copy.ext = match serde_json::to_value(out_ext) {
                Ok(v) => Some(v),
                Err(_) => { errs.push(BidderError::BadInput(format!("failed to build ext for impression id '{}'", imp.id))); continue; }
            };
            valid_imps.push(imp_copy);
        }

        if valid_imps.is_empty() {
            return (vec![], errs);
        }

        // Build URI with member_id if needed
        let uri = if unique_member_id != 0 {
            if self.endpoint.contains('?') {
                format!("{}&member_id={}", self.endpoint, unique_member_id)
            } else {
                format!("{}?member_id={}", self.endpoint, unique_member_id)
            }
        } else {
            self.endpoint.clone()
        };

        // Split into chunks of MAX_IMPS_PER_REQ
        let mut requests = Vec::new();
        let chunks: Vec<&[openrtb::Imp]> = valid_imps.chunks(MAX_IMPS_PER_REQ).collect();
        for chunk in chunks {
            let mut req_copy = request.clone();
            req_copy.imp = chunk.to_vec();
            let imp_ids = req_copy.imp.iter().map(|i| i.id.clone()).collect();
            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); return (vec![], errs); }
            };
            requests.push(RequestData { method: "POST".to_string(), uri: uri.clone(), body, headers: headers.clone(), imp_ids });
        }

        (requests, errs)
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(5);
        if let Some(cur) = &bid_resp.cur { if !cur.is_empty() { result.currency = cur.clone(); } }
        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_ext: BidExt = bid.ext.as_ref()
                    .and_then(|e| serde_json::from_value(e.clone()).ok())
                    .unwrap_or_default();
                match get_media_type_for_bid(bid_ext.appnexus.bid_type) {
                    Ok(bid_type) => {
                        let duration = bid_ext.appnexus.creative_info.video.duration;
                        let mut typed_bid = TypedBid::new(bid, bid_type);
                        typed_bid.bid_video = Some(ExtBidPrebidVideo { duration, primary_category: String::new() });
                        typed_bid.deal_priority = bid_ext.appnexus.deal_priority;
                        result.bids.push(typed_bid);
                    }
                    Err(e) => errs.push(e),
                }
            }
        }
        if !errs.is_empty() && result.bids.is_empty() { return Err(errs); }
        Ok(result)
    }
}
