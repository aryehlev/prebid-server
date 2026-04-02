use std::collections::HashMap;

use pbs_adapters::{
    Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid,
    get_imp_ids,
};
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

/// 33Across imp ext bidder params
#[derive(Debug, Default, Deserialize)]
struct ExtImp33Across {
    #[serde(rename = "productId", default)]
    product_id: String,
    #[serde(rename = "siteId", default)]
    site_id: String,
    #[serde(rename = "zoneId", default)]
    zone_id: String,
}

#[derive(Debug, Deserialize)]
struct ImpExt {
    bidder: ExtImp33Across,
}

/// 33Across imp ext for outgoing request
#[derive(Debug, Serialize)]
struct ImpTtxExt {
    ttx: ImpTtxExtInner,
}

#[derive(Debug, Serialize)]
struct ImpTtxExtInner {
    prod: String,
    #[serde(skip_serializing_if = "str::is_empty")]
    zoneid: String,
}

/// 33Across bid ext for media type
#[derive(Debug, Default, Deserialize)]
struct BidTtxExt {
    ttx: Option<BidTtxExtInner>,
}

#[derive(Debug, Default, Deserialize)]
struct BidTtxExtInner {
    #[serde(rename = "mediaType", default)]
    media_type: String,
}

/// 33Across request ext
#[derive(Debug, Default, Deserialize, Serialize)]
struct ReqTtxExt {
    ttx: Option<ReqTtxExtInner>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct ReqTtxExtInner {
    caller: Vec<TtxCaller>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TtxCaller {
    name: String,
    version: String,
}

const TTX_CALLER: TtxCaller = TtxCaller {
    name: String::new(),
    version: String::new(),
};

fn make_req_ext(request: &openrtb::BidRequest) -> Value {
    let caller = serde_json::json!({"name": "Prebid-Server", "version": "n/a"});
    let mut req_ext: ReqTtxExt = request.ext.as_ref()
        .and_then(|e| serde_json::from_value(e.clone()).ok())
        .unwrap_or_default();

    if req_ext.ttx.is_none() {
        req_ext.ttx = Some(ReqTtxExtInner { caller: Vec::new() });
    }

    if let Some(ttx) = &mut req_ext.ttx {
        ttx.caller.push(TtxCaller {
            name: "Prebid-Server".to_string(),
            version: "n/a".to_string(),
        });
    }

    serde_json::to_value(&req_ext).unwrap_or(Value::Null)
}

fn get_bid_type_from_ext(ext: &Option<Value>) -> BidType {
    if let Some(ext_val) = ext {
        if let Ok(bid_ext) = serde_json::from_value::<BidTtxExt>(ext_val.clone()) {
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
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        // Group imps by prod+zoneid key
        let mut grouped: HashMap<String, Vec<openrtb::Imp>> = HashMap::new();

        let req_ext = make_req_ext(request);

        for imp in &request.imp {
            // Validate: must have banner or video
            if imp.banner.is_none() && imp.video.is_none() {
                errs.push(BidderError::BadInput(format!(
                    "Imp ID {} must have at least one of [Banner, Video] defined",
                    imp.id
                )));
                continue;
            }

            let imp_ext: ImpExt = match imp
                .ext
                .as_ref()
                .and_then(|e| serde_json::from_value(e.clone()).ok())
            {
                Some(e) => e,
                None => {
                    errs.push(BidderError::BadInput(
                        "Failed to parse 33across imp ext".to_string(),
                    ));
                    continue;
                }
            };

            let ttx_ext = &imp_ext.bidder;
            let zone_id = if !ttx_ext.zone_id.is_empty() {
                ttx_ext.zone_id.clone()
            } else {
                ttx_ext.site_id.clone()
            };

            let imp_ttx_ext = ImpTtxExt {
                ttx: ImpTtxExtInner {
                    prod: ttx_ext.product_id.clone(),
                    zoneid: zone_id.clone(),
                },
            };

            let mut imp_copy = imp.clone();
            imp_copy.ext = serde_json::to_value(&imp_ttx_ext).ok();

            // Validate video if present
            if let Some(video) = &imp_copy.video {
                let w = video.w.unwrap_or(0);
                let h = video.h.unwrap_or(0);
                let protocols_ok = video.protocols.as_ref().map_or(false, |p| !p.is_empty());
                let mimes_ok = video.mimes.as_ref().map_or(false, |m| !m.is_empty());
                let playback_ok = video.playbackmethod.as_ref().map_or(false, |p| !p.is_empty());

                if w == 0 || h == 0 || !protocols_ok || !mimes_ok || !playback_ok {
                    errs.push(BidderError::BadInput(
                        "One or more invalid or missing video field(s) w, h, protocols, mimes, playbackmethod".to_string(),
                    ));
                    continue;
                }
            }

            let group_key = format!("{}{}", ttx_ext.product_id, zone_id);
            grouped.entry(group_key).or_default().push(imp_copy);
        }

        let mut requests = Vec::new();
        let mut headers = HashMap::new();
        headers.insert(
            "Content-Type".to_string(),
            "application/json;charset=utf-8".to_string(),
        );

        for (_, imp_list) in grouped {
            let mut req_copy = request.clone();
            req_copy.imp = imp_list;
            req_copy.ext = Some(req_ext.clone());

            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(BidderError::BadInput(e.to_string()));
                    continue;
                }
            };

            let imp_ids = get_imp_ids(&req_copy.imp);
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

        let bid_response: openrtb::BidResponse =
            serde_json::from_slice(&response.body)
                .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(1);

        for seat_bid in bid_response.seatbid {
            for bid in seat_bid.bid {
                let bid_type = get_bid_type_from_ext(&bid.ext);
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }

        Ok(result)
    }
}
