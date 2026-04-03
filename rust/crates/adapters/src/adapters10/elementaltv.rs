use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::{BidType, ExtBidPrebidVideo};
use serde::Deserialize;

pub struct ElementaltvAdapter {
    pub endpoint: String,
}

impl ElementaltvAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

/// ElementalTV bidder extension from imp.ext.bidder
#[derive(Debug, Default, Deserialize)]
struct ExtImpElementalTV {
    #[serde(rename = "adUnit", default)]
    ad_unit: String,
}

/// ElementalTV bid extension containing video duration
#[derive(Debug, Default, Deserialize)]
struct AdsImpExt {
    video: Option<AdsVideoExt>,
}

#[derive(Debug, Default, Deserialize)]
struct AdsVideoExt {
    duration: i32,
}

#[derive(Debug, Default, Deserialize)]
struct BidExtWrapper {
    ads: Option<AdsImpExt>,
}

fn unmarshal_ext(ext: &serde_json::Value) -> Result<ExtImpElementalTV, BidderError> {
    let bidder = ext.get("bidder")
        .ok_or_else(|| BidderError::BadInput("$.imp.ext.bidder required".to_string()))?;

    let ads_ext: ExtImpElementalTV = serde_json::from_value(bidder.clone())
        .map_err(|e| BidderError::BadInput(e.to_string()))?;

    if ads_ext.ad_unit.is_empty() {
        return Err(BidderError::BadInput("$.imp.ext.bidder.adunit required".to_string()));
    }

    Ok(ads_ext)
}

fn build_bid_uri(endpoint: &str, ad_unit: &str) -> String {
    // Replace {{.AdUnit}} macro in endpoint template
    let encoded = percent_encode(ad_unit);
    endpoint.replace("{{.AdUnit}}", &encoded)
}

fn percent_encode(s: &str) -> String {
    let mut result = String::new();
    for c in s.chars() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => result.push(c),
            _ => {
                for byte in c.to_string().as_bytes() {
                    result.push_str(&format!("%{:02X}", byte));
                }
            }
        }
    }
    result
}

impl Bidder for ElementaltvAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![]);
        }

        let mut requests = Vec::new();
        let mut errs = Vec::new();

        let mut headers = HashMap::new();
        headers.insert("Accept".to_string(), "application/json".to_string());
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("X-OpenRTB-Version".to_string(), "2.6".to_string());

        for imp in &request.imp {
            let imp_ext = match imp.ext.as_ref() {
                Some(e) => e,
                None => {
                    errs.push(BidderError::BadInput(format!(
                        "missing ext for imp id={}", imp.id
                    )));
                    continue;
                }
            };

            let ext = match unmarshal_ext(imp_ext) {
                Ok(e) => e,
                Err(e) => {
                    errs.push(e);
                    continue;
                }
            };

            // Create a per-imp request with a unique ID
            let mut per_req = request.clone();
            per_req.id = format!("{}-{}", request.id, ext.ad_unit);
            per_req.imp = vec![imp.clone()];

            let body = match serde_json::to_vec(&per_req) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(BidderError::BadInput(e.to_string()));
                    continue;
                }
            };

            let uri = build_bid_uri(&self.endpoint, &ext.ad_unit);

            requests.push(RequestData {
                method: "POST".to_string(),
                uri,
                body,
                headers: headers.clone(),
                imp_ids: get_imp_ids(&per_req.imp),
            });
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
            return Err(vec![BidderError::BadInput("bad request".to_string())]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "unexpected status: {}", response.status_code
            ))]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(format!(
                "invalid body: {}", e
            ))])?;

        // Build a map of imp ID -> bid type
        let mut imp_types: HashMap<String, BidType> = HashMap::new();
        for imp in &internal.imp {
            if imp_types.contains_key(&imp.id) {
                return Err(vec![BidderError::BadInput(format!(
                    "duplicate $.imp.id {}", imp.id
                ))]);
            }
            let bid_type = if imp.banner.is_some() {
                BidType::Banner
            } else if imp.video.is_some() {
                BidType::Video
            } else if imp.audio.is_some() {
                BidType::Audio
            } else if imp.native.is_some() {
                BidType::Native
            } else {
                return Err(vec![BidderError::BadInput(
                    "one of $.imp.banner, $.imp.video, $.imp.audio and $.imp.native field required".to_string()
                )]);
            };
            imp_types.insert(imp.id.clone(), bid_type);
        }

        let mut bids = Vec::new();
        for seat_bid in bid_resp.seatbid {
            for bid in seat_bid.bid {
                let tp = match imp_types.get(&bid.impid) {
                    Some(t) => t.clone(),
                    None => {
                        return Err(vec![BidderError::BadServerResponse(format!(
                            "unknown impid: {}", bid.impid
                        ))]);
                    }
                };

                let bid_video = if tp == BidType::Video {
                    // Parse bid.ext.ads.video for duration
                    let ads_ext = bid.ext.as_ref()
                        .and_then(|e| serde_json::from_value::<BidExtWrapper>(e.clone()).ok())
                        .and_then(|w| w.ads);

                    match ads_ext {
                        Some(ads) => {
                            match ads.video {
                                Some(v) => {
                                    let primary_category = bid.cat.as_ref()
                                        .and_then(|c| c.first())
                                        .cloned()
                                        .unwrap_or_default();
                                    Some(ExtBidPrebidVideo {
                                        duration: v.duration,
                                        primary_category,
                                    })
                                }
                                None => {
                                    return Err(vec![BidderError::BadServerResponse(
                                        "$.seatbid.bid.ext.ads.video required".to_string()
                                    )]);
                                }
                            }
                        }
                        None => {
                            return Err(vec![BidderError::BadServerResponse(
                                "$.seatbid.bid.ext.ads.video required".to_string()
                            )]);
                        }
                    }
                } else {
                    None
                };

                let mut typed_bid = TypedBid::new(bid, tp);
                typed_bid.bid_video = bid_video;
                bids.push(typed_bid);
            }
        }

        let mut result = BidderResponse::with_capacity(bids.len());
        result.bids = bids;
        Ok(result)
    }
}
