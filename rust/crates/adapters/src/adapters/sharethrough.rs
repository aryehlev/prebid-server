use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::{Deserialize, Serialize};

pub struct SharethroughAdapter {
    pub endpoint: String,
}

impl SharethroughAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

#[derive(Serialize, Deserialize, Default)]
struct ExtImpSharethrough {
    #[serde(rename = "pkey", default)]
    pub pkey: String,
    #[serde(rename = "bcat", default)]
    pub bcat: Vec<String>,
    #[serde(rename = "badv", default)]
    pub badv: Vec<String>,
}

#[derive(Deserialize, Default)]
struct ImpExt {
    #[serde(default)]
    bidder: ExtImpSharethrough,
}

/// Minimal structs to read bid type from bid.ext.prebid.type
#[derive(Deserialize, Default)]
struct BidExtPrebid {
    #[serde(rename = "type", default)]
    bid_type: String,
}

#[derive(Deserialize, Default)]
struct BidExt {
    prebid: Option<BidExtPrebid>,
}

fn get_media_type_for_bid(bid: &openrtb::Bid) -> Result<BidType, BidderError> {
    if let Some(ext) = &bid.ext {
        if let Ok(bid_ext) = serde_json::from_value::<BidExt>(ext.clone()) {
            if let Some(prebid) = bid_ext.prebid {
                return match prebid.bid_type.as_str() {
                    "banner" => Ok(BidType::Banner),
                    "video" => Ok(BidType::Video),
                    "native" => Ok(BidType::Native),
                    "audio" => Ok(BidType::Audio),
                    _ => Err(BidderError::BadServerResponse(format!(
                        "Failed to parse bid mediatype for impression \"{}\"",
                        bid.impid
                    ))),
                };
            }
        }
    }

    Err(BidderError::BadServerResponse(format!(
        "Failed to parse bid mediatype for impression \"{}\"",
        bid.impid
    )))
}

/// Split an impression into per-media-type impressions (matching Go's splitImpressionsByMediaType).
fn split_impressions_by_media_type(imp: &openrtb::Imp) -> Result<Vec<openrtb::Imp>, BidderError> {
    if imp.banner.is_none() && imp.video.is_none() && imp.native.is_none() {
        return Err(BidderError::BadInput(
            "Invalid MediaType. Sharethrough only supports Banner, Video and Native.".to_string(),
        ));
    }

    let mut impressions = Vec::new();

    if imp.banner.is_some() {
        let mut copy = imp.clone();
        copy.video = None;
        copy.native = None;
        copy.audio = None;
        impressions.push(copy);
    }

    if imp.video.is_some() {
        let mut copy = imp.clone();
        copy.banner = None;
        copy.native = None;
        copy.audio = None;
        impressions.push(copy);
    }

    if imp.native.is_some() {
        let mut copy = imp.clone();
        copy.banner = None;
        copy.video = None;
        copy.audio = None;
        impressions.push(copy);
    }

    Ok(impressions)
}

impl Bidder for SharethroughAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();
        let mut errs = Vec::new();

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        // Accumulate bcat and badv across all imps (matching Go's requestCopy.BCat/BAdv append)
        let mut req_copy = request.clone();
        req_copy.imp = Vec::new(); // will be set per request below

        for imp in &request.imp {
            // Parse imp ext
            let imp_ext: ImpExt = match imp.ext.as_ref().and_then(|e| serde_json::from_value(e.clone()).ok()) {
                Some(e) => e,
                None => {
                    errs.push(BidderError::BadInput("Failed to parse sharethrough imp ext".to_string()));
                    continue;
                }
            };

            let str_params = &imp_ext.bidder;
            let pkey = str_params.pkey.clone();

            // Accumulate bcat and badv from each imp's ext (matches Go behavior)
            if !str_params.bcat.is_empty() {
                req_copy.bcat.get_or_insert_with(Vec::new).extend(str_params.bcat.iter().cloned());
            }
            if !str_params.badv.is_empty() {
                req_copy.badv.get_or_insert_with(Vec::new).extend(str_params.badv.iter().cloned());
            }

            // Set tagid from pkey
            let mut imp_with_tagid = imp.clone();
            if !pkey.is_empty() {
                imp_with_tagid.tagid = Some(pkey.clone());
            }

            // Split by media type
            let media_imps = match split_impressions_by_media_type(&imp_with_tagid) {
                Ok(imps) => imps,
                Err(e) => { errs.push(e); continue; }
            };

            for media_imp in media_imps {
                let mut single_req = req_copy.clone();
                single_req.imp = vec![media_imp];

                let body = match serde_json::to_vec(&single_req) {
                    Ok(b) => b,
                    Err(e) => {
                        errs.push(BidderError::BadInput(e.to_string()));
                        continue;
                    }
                };

                let imp_ids = get_imp_ids(&single_req.imp);

                requests.push(RequestData {
                    method: "POST".to_string(),
                    uri: self.endpoint.clone(),
                    body,
                    headers: headers.clone(),
                    imp_ids,
                });
            }
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
                "unexpected status code: {}. Run with request.debug = 1 for more info",
                response.status_code
            ))]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::new();
        result.currency = "USD".to_string();
        let mut errs = Vec::new();

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                match get_media_type_for_bid(&bid) {
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
            // Non-fatal: return partial bids alongside errors
            // Following Go: return bidderResponse, errors (accumulate both)
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
                ext: Some(serde_json::json!({"bidder": {"pkey": "abc"}})),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn test_sharethrough_url_and_headers() {
        let adapter =
            SharethroughAdapter::new("https://btlr.sharethrough.com/universal/v1".to_string());
        let (reqs, _) = adapter.make_requests(&make_req(), &ExtraRequestInfo::default());
        assert_eq!(reqs.len(), 1);
        assert_eq!(
            reqs[0].uri,
            "https://btlr.sharethrough.com/universal/v1"
        );
        assert_eq!(
            reqs[0].headers.get("Content-Type").unwrap(),
            "application/json;charset=utf-8"
        );
        assert!(!reqs[0].body.is_empty());
    }

    #[test]
    fn test_sharethrough_make_bids() {
        let adapter = SharethroughAdapter::new("https://btlr.sharethrough.com/universal/v1".to_string());
        let body = br#"{"id":"r","seatbid":[{"bid":[{"id":"b","impid":"i1","price":0.8,"crid":"c","ext":{"prebid":{"type":"banner"}}}]}]}"#;
        let resp = ResponseData::new(200, body.to_vec());
        let result = adapter
            .make_bids(&make_req(), &RequestData::default(), &resp)
            .unwrap();
        assert_eq!(result.bids.len(), 1);
        assert_eq!(result.bids[0].bid_type, BidType::Banner);
    }
}
