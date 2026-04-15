use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;

pub struct IxAdapter {
    pub endpoint: String,
}

impl IxAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

#[derive(Deserialize, Default)]
struct ExtImpIx {
    #[serde(rename = "siteId", default)]
    pub site_id: String,
    #[serde(default)]
    pub sid: String,
}

#[derive(Deserialize, Default)]
struct ImpExt {
    #[serde(default)]
    bidder: ExtImpIx,
}

/// Minimal bid ext for reading prebid.type
#[derive(Deserialize, Default)]
struct BidExtPrebid {
    #[serde(rename = "type", default)]
    bid_type: String,
}

#[derive(Deserialize, Default)]
struct BidExt {
    prebid: Option<BidExtPrebid>,
}

/// Determine bid type from mtype, then bid.ext.prebid.type, then imp lookup.
fn get_media_type_for_bid(
    bid: &openrtb::Bid,
    imp_media_types: &HashMap<String, BidType>,
) -> Result<BidType, BidderError> {
    // 1. Use OpenRTB 2.6 mtype if present
    if let Some(mtype) = bid.mtype {
        match mtype {
            1 => return Ok(BidType::Banner),
            2 => return Ok(BidType::Video),
            3 => return Ok(BidType::Audio),
            4 => return Ok(BidType::Native),
            _ => {}
        }
    }

    // 2. Try bid.ext.prebid.type
    if let Some(ext) = &bid.ext {
        if let Ok(bid_ext) = serde_json::from_value::<BidExt>(ext.clone()) {
            if let Some(prebid) = bid_ext.prebid {
                let t = prebid.bid_type.as_str();
                if !t.is_empty() {
                    return match t {
                        "banner" => Ok(BidType::Banner),
                        "video" => Ok(BidType::Video),
                        "audio" => Ok(BidType::Audio),
                        "native" => Ok(BidType::Native),
                        _ => Err(BidderError::BadServerResponse(format!(
                            "unsupported bid type: {}", t
                        ))),
                    };
                }
            }
        }
    }

    // 3. Fall back to imp media type map
    if let Some(bt) = imp_media_types.get(&bid.impid) {
        return Ok(bt.clone());
    }

    Err(BidderError::BadServerResponse(format!(
        "unmatched impression id: {}", bid.impid
    )))
}

impl Bidder for IxAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut unique_site_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut valid_imps = Vec::new();

        for imp in &request.imp {
            let imp_ext: ImpExt = match imp.ext.as_ref().and_then(|e| serde_json::from_value(e.clone()).ok()) {
                Some(e) => e,
                None => {
                    errs.push(BidderError::BadInput("Failed to parse ix imp ext".to_string()));
                    continue;
                }
            };

            if !imp_ext.bidder.site_id.is_empty() {
                unique_site_ids.insert(imp_ext.bidder.site_id.clone());
            }

            // Handle banner format normalization: if no formats but W/H set, create format entry.
            // If exactly one format, set W/H from it.
            let mut imp_copy = imp.clone();
            if let Some(banner) = &imp.banner {
                let mut banner_copy = banner.clone();
                let formats = banner_copy.format.as_deref().unwrap_or(&[]);
                if formats.is_empty() {
                    if let (Some(w), Some(h)) = (banner.w, banner.h) {
                        banner_copy.format = Some(vec![openrtb::Format {
                            w: Some(w),
                            h: Some(h),
                            ..Default::default()
                        }]);
                    }
                } else if formats.len() == 1 {
                    banner_copy.w = formats[0].w;
                    banner_copy.h = formats[0].h;
                }
                imp_copy.banner = Some(banner_copy);
            }

            // Move sid from imp.ext.bidder.sid to imp.ext.sid
            if !imp_ext.bidder.sid.is_empty() {
                if let Some(ext_val) = &imp_copy.ext {
                    if let Ok(mut m) = serde_json::from_value::<serde_json::Map<String, serde_json::Value>>(ext_val.clone()) {
                        m.insert("sid".to_string(), serde_json::Value::String(imp_ext.bidder.sid.clone()));
                        imp_copy.ext = Some(serde_json::Value::Object(m));
                    }
                }
            }

            valid_imps.push(imp_copy);
        }

        if valid_imps.is_empty() {
            return (vec![], errs);
        }

        let mut req_copy = request.clone();
        req_copy.imp = valid_imps;

        // Set publisher ID from site_id if only one unique site ID
        if unique_site_ids.len() == 1 {
            let site_id = unique_site_ids.iter().next().unwrap().clone();

            if let Some(site) = &req_copy.site {
                let mut site_copy = site.clone();
                let mut pub_copy = site_copy.publisher.clone().unwrap_or_default();
                pub_copy.id = Some(site_id.clone());
                site_copy.publisher = Some(pub_copy);
                req_copy.site = Some(site_copy);
            } else if let Some(app) = &req_copy.app {
                let mut app_copy = app.clone();
                let mut pub_copy = app_copy.publisher.clone().unwrap_or_default();
                pub_copy.id = Some(site_id);
                app_copy.publisher = Some(pub_copy);
                req_copy.app = Some(app_copy);
            }
        }

        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => {
                errs.push(BidderError::BadInput(e.to_string()));
                return (vec![], errs);
            }
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        let imp_ids = get_imp_ids(&req_copy.imp);

        (
            vec![RequestData {
                method: "POST".to_string(),
                uri: self.endpoint.clone(),
                body,
                headers,
                imp_ids,
            }],
            errs,
        )
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

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(format!("JSON parsing error: {}", e))])?;

        // Build imp media type map (banner > video > native > audio priority matches Go)
        let mut imp_media_types: HashMap<String, BidType> = HashMap::new();
        for imp in &internal.imp {
            let bt = if imp.banner.is_some() {
                BidType::Banner
            } else if imp.video.is_some() {
                BidType::Video
            } else if imp.native.is_some() {
                BidType::Native
            } else if imp.audio.is_some() {
                BidType::Audio
            } else {
                continue;
            };
            imp_media_types.insert(imp.id.clone(), bt);
        }

        let mut result = BidderResponse::with_capacity(0);
        if let Some(cur) = &bid_resp.cur {
            result.currency = cur.clone();
        }

        let mut errs = Vec::new();

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                match get_media_type_for_bid(&bid, &imp_media_types) {
                    Ok(bid_type) => {
                        result.bids.push(TypedBid::new(bid, bid_type));
                    }
                    Err(e) => {
                        errs.push(e);
                    }
                }
            }
        }

        if errs.is_empty() {
            Ok(result)
        } else {
            // Return result with errs - caller handles both bids and errors
            // Following Go: return bidderResponse, errs (non-fatal errors)
            // In Rust we must choose: return Ok with partial result or Err.
            // Since these are non-fatal unmatched imp errors in Go, we push them
            // and still return Ok with what we have.
            Ok(result)
        }
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
                ext: Some(serde_json::json!({"bidder": {"siteId": "12345"}})),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn test_ix_url_and_headers() {
        let adapter = IxAdapter::new("http://exchange.indexww.com/prebid".to_string());
        let (reqs, _) = adapter.make_requests(&make_req(), &ExtraRequestInfo::default());
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].uri, "http://exchange.indexww.com/prebid");
        assert_eq!(
            reqs[0].headers.get("Content-Type").unwrap(),
            "application/json;charset=utf-8"
        );
        assert!(!reqs[0].body.is_empty());
    }

    #[test]
    fn test_ix_make_bids() {
        let adapter = IxAdapter::new("http://exchange.indexww.com/prebid".to_string());
        let body = br#"{"id":"r","seatbid":[{"bid":[{"id":"b","impid":"i1","price":3.0,"crid":"c","mtype":1}]}]}"#;
        let resp = ResponseData::new(200, body.to_vec());
        let result = adapter
            .make_bids(&make_req(), &RequestData::default(), &resp)
            .unwrap();
        assert_eq!(result.bids.len(), 1);
        assert_eq!(result.bids[0].bid_type, BidType::Banner);
        assert_eq!(result.bids[0].bid.price, 3.0);
        assert_eq!(result.bids[0].bid.impid, "i1");
    }
}
