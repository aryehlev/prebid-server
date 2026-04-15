use std::collections::HashMap;

use crate::{
    Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid,
    get_imp_ids,
};
use openrtb_ext::{BidType, ExtBidPrebidMeta, ExtBidPrebidVideo, FledgeAuctionConfig};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const HB_CONFIG: &str = "hb_pbs_1.0.0";

pub struct OpenxAdapter {
    pub endpoint: String,
    pub bidder_name: String,
}

impl OpenxAdapter {
    pub fn new(endpoint: String, bidder_name: String) -> Self {
        Self {
            endpoint,
            bidder_name,
        }
    }
}

/// OpenX imp ext bidder params
#[derive(Debug, Default, Deserialize)]
struct ExtImpOpenx {
    #[serde(rename = "delDomain", default)]
    del_domain: String,
    #[serde(default)]
    platform: String,
    #[serde(default)]
    unit: Value,
    #[serde(rename = "customFloor", default)]
    custom_floor: Value,
    #[serde(rename = "customParams", default)]
    custom_params: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct ImpExt {
    bidder: ExtImpOpenx,
}

/// OpenX request ext
#[derive(Debug, Serialize)]
struct OpenxReqExt {
    #[serde(rename = "delDomain", skip_serializing_if = "str::is_empty")]
    del_domain: String,
    #[serde(skip_serializing_if = "str::is_empty")]
    platform: String,
    #[serde(rename = "bc")]
    bidder_config: String,
}

/// OpenX response ext for FLEDGE auction configs
#[derive(Debug, Default, Deserialize)]
struct OpenxRespExt {
    #[serde(rename = "fledge_auction_configs", default)]
    fledge_auction_configs: HashMap<String, Value>,
}

/// OpenX bid ext
#[derive(Debug, Default, Deserialize)]
struct OxBidExt {
    #[serde(rename = "dsp_id", default)]
    dsp_id: i32,
    #[serde(rename = "brand_id", default)]
    brand_id: i32,
    #[serde(rename = "buyer_id", default)]
    buyer_id: String,
}

fn value_to_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse::<f64>().ok(),
        _ => None,
    }
}

fn value_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}

fn get_bid_type_for_imp(imp_id: &str, imps: &[openrtb::Imp]) -> BidType {
    for imp in imps {
        if imp.id == imp_id {
            if imp.banner.is_none() && imp.video.is_some() {
                return BidType::Video;
            } else if imp.banner.is_none() && imp.native.is_some() {
                return BidType::Native;
            }
            return BidType::Banner;
        }
    }
    BidType::Banner
}

fn preprocess_imp(imp: &openrtb::Imp, req_ext: &mut OpenxReqExt) -> Result<openrtb::Imp, BidderError> {
    let imp_ext: ImpExt = imp
        .ext
        .as_ref()
        .and_then(|e| serde_json::from_value(e.clone()).ok())
        .ok_or_else(|| BidderError::BadInput("Failed to parse openx imp ext".to_string()))?;

    let openx_ext = &imp_ext.bidder;

    req_ext.del_domain = openx_ext.del_domain.clone();
    req_ext.platform = openx_ext.platform.clone();

    let mut imp_copy = imp.clone();
    imp_copy.tagid = Some(value_to_string(&openx_ext.unit));

    if imp_copy.bidfloor.unwrap_or(0.0) == 0.0 {
        if let Some(floor) = value_to_f64(&openx_ext.custom_floor) {
            if floor > 0.0 {
                imp_copy.bidfloor = Some(floor);
            }
        }
    }

    // Strip bidder/prebid keys from imp ext, keep rest
    if let Some(ext_val) = &imp.ext {
        if let Ok(mut ext_map) = serde_json::from_value::<serde_json::Map<String, Value>>(ext_val.clone()) {
            ext_map.remove("prebid");
            ext_map.remove("bidder");

            if let Some(custom_params) = &openx_ext.custom_params {
                ext_map.insert("customParams".to_string(), custom_params.clone());
            }

            if ext_map.is_empty() {
                imp_copy.ext = None;
            } else {
                imp_copy.ext = Some(Value::Object(ext_map));
            }
        }
    }

    // Set video ext for rewarded
    if let Some(video) = &imp_copy.video {
        let mut video_copy = video.clone();
        if imp_copy.rwdd == Some(1) {
            video_copy.ext = Some(serde_json::json!({"rewarded": 1}));
        } else {
            video_copy.ext = None;
        }
        imp_copy.video = Some(video_copy);
    }

    Ok(imp_copy)
}

fn make_single_request(
    request: &openrtb::BidRequest,
    imps: Vec<openrtb::Imp>,
    endpoint: &str,
) -> Result<RequestData, Vec<BidderError>> {
    if imps.is_empty() {
        return Err(vec![]);
    }

    let mut req_ext = OpenxReqExt {
        del_domain: String::new(),
        platform: String::new(),
        bidder_config: HB_CONFIG.to_string(),
    };

    let mut valid_imps = Vec::new();
    let mut errs = Vec::new();

    for imp in imps {
        match preprocess_imp(&imp, &mut req_ext) {
            Ok(processed) => valid_imps.push(processed),
            Err(e) => errs.push(e),
        }
    }

    if valid_imps.is_empty() {
        return Err(errs);
    }

    let mut req_copy = request.clone();
    req_copy.imp = valid_imps;
    req_copy.ext = Some(serde_json::to_value(&req_ext).unwrap_or(Value::Null));

    let body = serde_json::to_vec(&req_copy)
        .map_err(|e| vec![BidderError::BadInput(e.to_string())])?;

    let imp_ids = get_imp_ids(&req_copy.imp);

    let mut headers = HashMap::new();
    headers.insert(
        "Content-Type".to_string(),
        "application/json;charset=utf-8".to_string(),
    );
    headers.insert("Accept".to_string(), "application/json".to_string());

    Ok(RequestData {
        method: "POST".to_string(),
        uri: endpoint.to_string(),
        body,
        headers,
        imp_ids,
    })
}

impl Bidder for OpenxAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut banner_and_native_imps = Vec::new();
        let mut video_imps = Vec::new();

        for imp in &request.imp {
            if imp.banner.is_some() {
                banner_and_native_imps.push(imp.clone());
            } else if imp.video.is_some() {
                video_imps.push(imp.clone());
            } else if imp.native.is_some() {
                banner_and_native_imps.push(imp.clone());
            }
        }

        let mut requests = Vec::new();
        let mut errs = Vec::new();

        // Banner + native request
        if !banner_and_native_imps.is_empty() {
            match make_single_request(request, banner_and_native_imps, &self.endpoint) {
                Ok(req) => requests.push(req),
                Err(mut e) => errs.append(&mut e),
            }
        }

        // One request per video imp
        for video_imp in video_imps {
            match make_single_request(request, vec![video_imp], &self.endpoint) {
                Ok(req) => requests.push(req),
                Err(mut e) => errs.append(&mut e),
            }
        }

        (requests, errs)
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

        let bid_response: openrtb::BidResponse =
            serde_json::from_slice(&response.body)
                .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(5);

        if let Some(cur) = &bid_response.cur {
            if !cur.is_empty() {
                result.currency = cur.clone();
            }
        }

        // Parse FLEDGE auction configs
        if let Some(ext) = &bid_response.ext {
            if let Ok(resp_ext) = serde_json::from_value::<OpenxRespExt>(ext.clone()) {
                for (imp_id, config) in resp_ext.fledge_auction_configs {
                    result.fledge_auction_configs.push(FledgeAuctionConfig {
                        impid: imp_id,
                        bidder: Some(self.bidder_name.clone()),
                        adapter: None,
                        config,
                    });
                }
            }
        }

        for seat_bid in bid_response.seatbid {
            for bid in seat_bid.bid {
                let bid_type = get_bid_type_for_imp(&bid.impid, &internal.imp);

                // Build video ext
                let primary_category = bid.cat.as_ref()
                    .and_then(|c| c.first())
                    .cloned()
                    .unwrap_or_default();
                let bid_video = Some(ExtBidPrebidVideo {
                    duration: 0, // openx uses bid.dur but we don't have it in the struct
                    primary_category,
                });

                // Parse bid meta
                let bid_meta = bid.ext.as_ref().and_then(|ext| {
                    serde_json::from_value::<OxBidExt>(ext.clone()).ok()
                }).and_then(|ox_ext| {
                    let buyer_id = ox_ext.buyer_id.parse::<i32>().unwrap_or(0);
                    if buyer_id <= 0 && ox_ext.dsp_id <= 0 && ox_ext.brand_id <= 0 {
                        None
                    } else {
                        Some(ExtBidPrebidMeta {
                            network_id: if ox_ext.dsp_id != 0 { Some(ox_ext.dsp_id) } else { None },
                            advertiser_id: if buyer_id != 0 { Some(buyer_id) } else { None },
                            brand_id: if ox_ext.brand_id != 0 { Some(ox_ext.brand_id) } else { None },
                            ..Default::default()
                        })
                    }
                });

                let mut typed_bid = TypedBid::new(bid, bid_type);
                typed_bid.bid_video = bid_video;
                typed_bid.bid_meta = bid_meta;
                result.bids.push(typed_bid);
            }
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_req() -> openrtb::BidRequest {
        openrtb::BidRequest {
            id: "r".to_string(),
            imp: vec![openrtb::Imp {
                id: "i1".to_string(),
                banner: Some(openrtb::Banner {
                    w: Some(300),
                    h: Some(250),
                    ..Default::default()
                }),
                ext: Some(serde_json::json!({
                    "bidder": {"delDomain": "se-demo-d.openx.net", "unit": "540949380"}
                })),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn test_openx_url_and_headers() {
        let adapter = OpenxAdapter::new(
            "https://rtb.openx.net/sync/prebid".to_string(),
            "openx".to_string(),
        );
        let (reqs, _) = adapter.make_requests(&make_req(), &ExtraRequestInfo::default());
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].uri, "https://rtb.openx.net/sync/prebid");
        assert_eq!(
            reqs[0].headers.get("Content-Type").unwrap(),
            "application/json;charset=utf-8"
        );
        assert!(!reqs[0].body.is_empty());
    }

    #[test]
    fn test_openx_make_bids() {
        let adapter = OpenxAdapter::new(
            "https://rtb.openx.net/sync/prebid".to_string(),
            "openx".to_string(),
        );
        let body = br#"{"id":"r","seatbid":[{"bid":[{"id":"b","impid":"i1","price":1.25,"crid":"c"}]}]}"#;
        let resp = ResponseData::new(200, body.to_vec());
        let result = adapter
            .make_bids(&make_req(), &RequestData::default(), &resp)
            .unwrap();
        assert_eq!(result.bids.len(), 1);
        assert_eq!(result.bids[0].bid.impid, "i1");
        assert_eq!(result.bids[0].bid.price, 1.25);
        assert_eq!(result.bids[0].bid.crid.as_deref(), Some("c"));
    }
}
