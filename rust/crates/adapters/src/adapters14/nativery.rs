use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use openrtb_ext::{BidType, ExtBidPrebidMeta};
use serde::{Deserialize, Serialize};

pub struct NativeryAdapter { pub endpoint: String }
impl NativeryAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct BidReqExtNativery {
    #[serde(rename = "isAmp")]
    is_amp: bool,
    #[serde(rename = "widgetId")]
    widget_id: String,
}

#[derive(Debug, Deserialize, Default)]
struct ImpExtNativery {
    #[serde(rename = "widgetId", default)]
    widget_id: String,
}

#[derive(Debug, Deserialize, Default)]
struct BidExtNativery {
    #[serde(rename = "bid_ad_media_type", default)]
    bid_type: String,
    #[serde(rename = "bid_adv_domains", default)]
    bid_adv_domains: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
struct BidExt {
    #[serde(default)]
    nativery: BidExtNativery,
}

fn get_media_type_for_bid(bid_ext: &BidExt) -> Result<BidType, BidderError> {
    match bid_ext.nativery.bid_type.as_str() {
        "native" => Ok(BidType::Native),
        "display" | "banner" | "rich_media" => Ok(BidType::Banner),
        "video" => Ok(BidType::Video),
        other => Err(BidderError::BadServerResponse(format!(
            "unrecognized bid_ad_media_type in response from nativery: {}", other
        ))),
    }
}

fn build_bid_meta(media_type: &str, adv_domains: Vec<String>) -> ExtBidPrebidMeta {
    ExtBidPrebidMeta {
        media_type: Some(media_type.to_string()),
        advertiser_domains: Some(adv_domains),
        ..Default::default()
    }
}

fn get_nativery_widget_id(imp: &openrtb::Imp) -> Result<String, BidderError> {
    let bidder_val = imp.ext.as_ref()
        .and_then(|e| e.get("bidder"))
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let nativery_ext: ImpExtNativery = serde_json::from_value(bidder_val)
        .map_err(|e| BidderError::BadInput(e.to_string()))?;
    Ok(nativery_ext.widget_id)
}

impl Bidder for NativeryAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, info: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();
        let mut errs = Vec::new();
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        let is_amp = info.pbs_entry_point == "amp";

        let mut widget_id = String::new();
        let mut valid_imps = Vec::new();
        for (i, imp) in request.imp.iter().enumerate() {
            match get_nativery_widget_id(imp) {
                Ok(wid) => {
                    if i == 0 { widget_id = wid; }
                    valid_imps.push(imp.clone());
                }
                Err(e) => errs.push(e),
            }
        }

        if valid_imps.is_empty() {
            return (vec![], errs);
        }

        let nativery_ext = BidReqExtNativery { is_amp, widget_id };
        let nativery_ext_val = match serde_json::to_value(&nativery_ext) {
            Ok(v) => v,
            Err(e) => { errs.push(BidderError::BadInput(e.to_string())); return (vec![], errs); }
        };

        for imp in valid_imps {
            let mut req_copy = request.clone();
            req_copy.imp = vec![imp.clone()];

            let mut ext_map: HashMap<String, serde_json::Value> = req_copy.ext
                .as_ref()
                .and_then(|e| serde_json::from_value(e.clone()).ok())
                .unwrap_or_default();
            ext_map.insert("nativery".to_string(), nativery_ext_val.clone());
            req_copy.ext = match serde_json::to_value(ext_map) {
                Ok(v) => Some(v),
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };

            let imp_ids = vec![imp.id.clone()];
            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };
            requests.push(RequestData { method: "POST".to_string(), uri: self.endpoint.clone(), body, headers: headers.clone(), imp_ids });
        }
        (requests, errs)
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 {
            if let Some(nativery_err) = response.headers.get("X-Nativery-Error") {
                if !nativery_err.is_empty() {
                    return Err(vec![BidderError::BadInput(format!("Nativery Error: {}.", nativery_err))]);
                }
            }
            return Err(vec![BidderError::BadServerResponse("No Content".to_string())]);
        }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(internal.imp.len());
        result.currency = bid_resp.cur.clone().unwrap_or_else(|| "EUR".to_string());
        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_ext: BidExt = bid.ext.as_ref()
                    .and_then(|e| serde_json::from_value(e.clone()).ok())
                    .unwrap_or_default();
                match get_media_type_for_bid(&bid_ext) {
                    Ok(bid_type) => {
                        let type_str = match bid_type { openrtb_ext::BidType::Banner => "banner", openrtb_ext::BidType::Video => "video", openrtb_ext::BidType::Native => "native", _ => "banner" };
                        let bid_meta = build_bid_meta(type_str, bid_ext.nativery.bid_adv_domains);
                        let mut typed_bid = TypedBid::new(bid, bid_type);
                        typed_bid.bid_meta = Some(bid_meta);
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
