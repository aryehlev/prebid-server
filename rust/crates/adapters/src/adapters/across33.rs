use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub struct Across33Adapter {
    pub endpoint: String,
}

impl Across33Adapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

/// 33Across bidder extension from imp.ext.bidder
#[derive(Debug, Default, Deserialize)]
struct ExtImp33Across {
    #[serde(rename = "productId", default)]
    product_id: String,
    #[serde(rename = "siteId", default)]
    site_id: String,
    #[serde(rename = "zoneId", default)]
    zone_id: String,
}

/// The TTX-style imp ext output
#[derive(Debug, Serialize, Deserialize)]
struct ImpTtxExt {
    ttx: TtxImpExtInner,
}

#[derive(Debug, Serialize, Deserialize)]
struct TtxImpExtInner {
    prod: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    zoneid: String,
}

/// Request extension for TTX caller tracking
#[derive(Debug, Default, Serialize, Deserialize)]
struct ReqExt {
    #[serde(skip_serializing_if = "Option::is_none")]
    ttx: Option<ReqTtxExt>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ReqTtxExt {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    caller: Vec<TtxCaller>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct TtxCaller {
    name: String,
    version: String,
}

/// Bid ext for determining media type
#[derive(Debug, Default, Deserialize)]
struct BidExt {
    ttx: Option<BidTtxExt>,
}

#[derive(Debug, Default, Deserialize)]
struct BidTtxExt {
    #[serde(rename = "mediaType", default)]
    media_type: String,
}

fn make_imps(imp: &openrtb::Imp) -> Result<openrtb::Imp, BidderError> {
    if imp.banner.is_none() && imp.video.is_none() {
        return Err(BidderError::BadInput(format!(
            "Imp ID {} must have at least one of [Banner, Video] defined", imp.id
        )));
    }

    let bidder_ext = imp.ext.as_ref()
        .and_then(|e| e.get("bidder"))
        .ok_or_else(|| BidderError::BadInput("missing bidder ext in imp".to_string()))?;

    let ttx_ext: ExtImp33Across = serde_json::from_value(bidder_ext.clone())
        .map_err(|e| BidderError::BadInput(e.to_string()))?;

    // Build the new imp ext
    let zoneid = if !ttx_ext.zone_id.is_empty() {
        ttx_ext.zone_id.clone()
    } else {
        ttx_ext.site_id.clone()
    };

    let imp_ext = ImpTtxExt {
        ttx: TtxImpExtInner {
            prod: ttx_ext.product_id.clone(),
            zoneid,
        },
    };

    let mut imp_copy = imp.clone();
    imp_copy.ext = Some(serde_json::to_value(&imp_ext)
        .map_err(|e| BidderError::BadInput(e.to_string()))?);

    // Validate video parameters if present
    if let Some(video) = &imp.video {
        let video_copy = validate_video_params(video, &ttx_ext.product_id)?;
        imp_copy.video = Some(video_copy);
    }

    Ok(imp_copy)
}

fn validate_video_params(video: &openrtb::Video, prod: &str) -> Result<openrtb::Video, BidderError> {
    let w = video.w.unwrap_or(0);
    let h = video.h.unwrap_or(0);

    if w == 0 || h == 0 || video.protocols.is_none() || video.mimes.is_none() || video.playbackmethod.is_none() {
        return Err(BidderError::BadInput(
            "One or more invalid or missing video field(s) w, h, protocols, mimes, playbackmethod".to_string()
        ));
    }

    let mut video_copy = video.clone();

    // For instream, set startdelay if not present
    if prod == "instream" && video_copy.startdelay.is_none() {
        video_copy.startdelay = Some(0);
    }

    Ok(video_copy)
}

fn make_req_ext(request: &openrtb::BidRequest) -> Result<Value, BidderError> {
    let mut req_ext: ReqExt = if let Some(ext) = &request.ext {
        serde_json::from_value(ext.clone()).unwrap_or_default()
    } else {
        ReqExt::default()
    };

    let ttx = req_ext.ttx.get_or_insert_with(|| ReqTtxExt { caller: Vec::new() });
    ttx.caller.push(TtxCaller {
        name: "Prebid-Server".to_string(),
        version: "n/a".to_string(),
    });

    serde_json::to_value(&req_ext).map_err(|e| BidderError::BadInput(e.to_string()))
}

fn get_bid_type_from_ext(ext: &Option<Value>) -> BidType {
    if let Some(ext_val) = ext {
        if let Ok(bid_ext) = serde_json::from_value::<BidExt>(ext_val.clone()) {
            if let Some(ttx) = bid_ext.ttx {
                if ttx.media_type == "video" {
                    return BidType::Video;
                }
            }
        }
    }
    BidType::Banner
}

impl Bidder for Across33Adapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut grouped_imps: HashMap<String, Vec<openrtb::Imp>> = HashMap::new();

        // Build the request extension with caller info
        let req_ext = match make_req_ext(request) {
            Ok(e) => e,
            Err(e) => {
                errs.push(e);
                // Not blocking - continue with no ext
                Value::Null
            }
        };

        let mut req = request.clone();
        if req_ext != Value::Null {
            req.ext = Some(req_ext);
        }

        // Process each imp, group by prod+zoneid key
        for imp in &request.imp {
            match make_imps(imp) {
                Ok(imp_copy) => {
                    // Get the key from the new ext
                    let key = if let Some(ext) = &imp_copy.ext {
                        if let Ok(ttx_ext) = serde_json::from_value::<ImpTtxExt>(ext.clone()) {
                            format!("{}{}", ttx_ext.ttx.prod, ttx_ext.ttx.zoneid)
                        } else {
                            String::new()
                        }
                    } else {
                        String::new()
                    };
                    grouped_imps.entry(key).or_default().push(imp_copy);
                }
                Err(e) => {
                    errs.push(e);
                }
            }
        }

        let mut requests = Vec::new();
        for (_key, imp_list) in grouped_imps {
            let mut req_copy = req.clone();
            req_copy.imp = imp_list;

            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(BidderError::BadInput(e.to_string()));
                    continue;
                }
            };

            let mut headers = HashMap::new();
            headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());

            let imp_ids = get_imp_ids(&req_copy.imp);
            requests.push(RequestData {
                method: "POST".to_string(),
                uri: self.endpoint.clone(),
                body,
                headers,
                imp_ids,
            });
        }

        (requests, errs)
    }

    fn make_bids(
        &self,
        _internal: &openrtb::BidRequest,
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

        let bid_response: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(1);

        for sb in bid_response.seatbid {
            for bid in sb.bid {
                let bid_type = get_bid_type_from_ext(&bid.ext);
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }

        Ok(result)
    }
}
