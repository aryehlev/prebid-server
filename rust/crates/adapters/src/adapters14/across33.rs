use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use openrtb_ext::BidType;
use serde::{Deserialize, Serialize};

pub struct Across33Adapter { pub endpoint: String }
impl Across33Adapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct ImpExtTtx {
    ttx: ImpTtxExt,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
struct ImpTtxExt {
    #[serde(default)]
    prod: String,
    #[serde(rename = "zoneid", default)]
    zoneid: String,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
struct Ext33across {
    #[serde(rename = "productId", default)]
    product_id: String,
    #[serde(rename = "siteId", default)]
    site_id: String,
    #[serde(rename = "zoneId", default)]
    zone_id: String,
}

#[derive(Debug, Deserialize, Clone)]
struct BidExtTtx {
    #[serde(default)]
    ttx: BidTtxExt,
}

#[derive(Debug, Deserialize, Clone, Default)]
struct BidTtxExt {
    #[serde(rename = "mediaType", default)]
    media_type: String,
}

fn get_bid_type(ext: &BidExtTtx) -> BidType {
    if ext.ttx.media_type == "video" {
        BidType::Video
    } else {
        BidType::Banner
    }
}

impl Bidder for Across33Adapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        // Group imps by prod+zoneid key
        let mut grouped: HashMap<String, Vec<openrtb::Imp>> = HashMap::new();

        for imp in &request.imp {
            if imp.banner.is_none() && imp.video.is_none() {
                errs.push(BidderError::BadInput(format!(
                    "Imp ID {} must have at least one of [Banner, Video] defined", imp.id
                )));
                continue;
            }

            let bidder_val = match imp.ext.as_ref().and_then(|e| e.get("bidder")) {
                Some(v) => v.clone(),
                None => {
                    errs.push(BidderError::BadInput("Missing bidder ext".to_string()));
                    continue;
                }
            };

            let ttx_ext: Ext33across = match serde_json::from_value(bidder_val) {
                Ok(e) => e,
                Err(e) => {
                    errs.push(BidderError::BadInput(e.to_string()));
                    continue;
                }
            };

            let zoneid = if !ttx_ext.zone_id.is_empty() {
                ttx_ext.zone_id.clone()
            } else {
                ttx_ext.site_id.clone()
            };

            let imp_ttx = ImpExtTtx {
                ttx: ImpTtxExt {
                    prod: ttx_ext.product_id.clone(),
                    zoneid: zoneid.clone(),
                },
            };

            let imp_ext_json = match serde_json::to_value(&imp_ttx) {
                Ok(v) => v,
                Err(e) => {
                    errs.push(BidderError::BadInput(e.to_string()));
                    continue;
                }
            };

            let mut imp_copy = imp.clone();

            // Validate video if present
            if let Some(ref video) = imp.video {
                if video.w.is_none() || video.h.is_none() || video.protocols.as_ref().map_or(true, |v| v.is_empty())
                    || video.mimes.as_ref().map_or(true, |v| v.is_empty()) || video.playbackmethod.as_ref().map_or(true, |v| v.is_empty())
                {
                    errs.push(BidderError::BadInput(
                        "One or more invalid or missing video field(s) w, h, protocols, mimes, playbackmethod".to_string()
                    ));
                    continue;
                }
            }

            imp_copy.ext = Some(imp_ext_json);

            let key = format!("{}{}", ttx_ext.product_id, zoneid);
            grouped.entry(key).or_default().push(imp_copy);
        }

        let headers: HashMap<String, String> = {
            let mut h = HashMap::new();
            h.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
            h
        };

        let mut requests = Vec::new();
        for (_key, imps) in grouped {
            let imp_ids: Vec<String> = imps.iter().map(|i| i.id.clone()).collect();
            let mut req_copy = request.clone();
            req_copy.imp = imps;

            // Build request ext with caller info
            let caller_entry = serde_json::json!({
                "ttx": {
                    "caller": [{"name": "Prebid-Server", "version": "n/a"}]
                }
            });
            let req_ext = if let Some(existing) = &req_copy.ext {
                let mut merged = existing.clone();
                if let Some(obj) = merged.as_object_mut() {
                    obj.insert("ttx".to_string(), caller_entry["ttx"].clone());
                }
                merged
            } else {
                caller_entry
            };
            req_copy.ext = Some(req_ext);

            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
            };

            requests.push(RequestData {
                method: "POST".to_string(),
                uri: self.endpoint.clone(),
                body,
                headers: headers.clone(),
                imp_ids,
            });
        }

        (requests, errs)
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code == 400 {
            return Err(vec![BidderError::BadInput(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info", response.status_code
            ))]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info", response.status_code
            ))]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(1);
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = bid.ext.as_ref()
                    .and_then(|e| serde_json::from_value::<BidExtTtx>(e.clone()).ok())
                    .map(|e| get_bid_type(&e))
                    .unwrap_or(BidType::Banner);
                result.bids.push(TypedBid::new(bid, bid_type));
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
                    "bidder": {"productId": "siab", "siteId": "site1", "zoneId": "zone1"}
                })),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn test_across33_url_and_headers() {
        let adapter = Across33Adapter::new("https://ssc.33across.com/api/v1/s2s".to_string());
        let (reqs, _) = adapter.make_requests(&make_req(), &ExtraRequestInfo::default());
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].uri, "https://ssc.33across.com/api/v1/s2s");
        assert_eq!(
            reqs[0].headers.get("Content-Type").unwrap(),
            "application/json;charset=utf-8"
        );
        assert!(!reqs[0].body.is_empty());
    }

    #[test]
    fn test_across33_make_bids() {
        let adapter = Across33Adapter::new("https://ssc.33across.com/api/v1/s2s".to_string());
        let body = br#"{"id":"r","seatbid":[{"bid":[{"id":"b","impid":"i1","price":1.1,"crid":"c","ext":{"ttx":{"mediaType":"banner"}}}]}]}"#;
        let resp = ResponseData::new(200, body.to_vec());
        let result = adapter
            .make_bids(&make_req(), &RequestData::default(), &resp)
            .unwrap();
        assert_eq!(result.bids.len(), 1);
        assert_eq!(result.bids[0].bid_type, BidType::Banner);
    }
}
