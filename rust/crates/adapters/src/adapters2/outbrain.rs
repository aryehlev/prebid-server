use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;
use serde_json::Value;

pub struct OutbrainAdapter { pub endpoint: String }
impl OutbrainAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

#[derive(Debug, Default, Deserialize)]
struct ExtImpOutbrainPublisher {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    domain: String,
}

#[derive(Debug, Default, Deserialize)]
struct ExtImpOutbrain {
    #[serde(rename = "tagId", default)]
    tag_id: String,
    #[serde(default)]
    publisher: ExtImpOutbrainPublisher,
    #[serde(rename = "bcat", default)]
    bcat: Option<Vec<String>>,
    #[serde(rename = "badv", default)]
    badv: Option<Vec<String>>,
}

fn get_media_type_for_imp(imp_id: &str, imps: &[openrtb::Imp]) -> Result<BidType, BidderError> {
    for imp in imps {
        if imp.id == imp_id {
            if imp.native.is_some() {
                return Ok(BidType::Native);
            } else if imp.banner.is_some() {
                return Ok(BidType::Banner);
            } else if imp.video.is_some() {
                return Ok(BidType::Video);
            }
        }
    }
    Err(BidderError::BadInput(format!(
        "Failed to find native/banner/video impression \"{}\" ", imp_id
    )))
}

/// Transform native event trackers to imptrackers/jstracker.
/// The native-trk.js library used by Outbrain doesn't support native 1.2 eventtrackers,
/// so we transform them to the deprecated imptrackers and jstracker fields.
fn transform_event_trackers(native_payload: &mut Value) {
    let event_trackers = match native_payload.get("eventtrackers") {
        Some(Value::Array(arr)) => arr.clone(),
        _ => return,
    };

    // event type 1 = impression
    // method 1 = image, method 2 = js
    for tracker in &event_trackers {
        let event = tracker.get("event").and_then(|v| v.as_i64()).unwrap_or(0);
        if event != 1 {
            continue;
        }
        let method = tracker.get("method").and_then(|v| v.as_i64()).unwrap_or(0);
        let url = match tracker.get("url").and_then(|v| v.as_str()) {
            Some(u) => u.to_string(),
            None => continue,
        };
        match method {
            1 => {
                // image tracker -> imptrackers
                if let Some(arr) = native_payload.get_mut("imptrackers").and_then(|v| v.as_array_mut()) {
                    arr.push(Value::String(url));
                } else {
                    native_payload["imptrackers"] = Value::Array(vec![Value::String(url)]);
                }
            }
            2 => {
                // js tracker -> jstracker
                native_payload["jstracker"] = Value::String(format!("<script src=\"{}\"></script>", url));
            }
            _ => {}
        }
    }

    // Remove eventtrackers
    if let Some(obj) = native_payload.as_object_mut() {
        obj.remove("eventtrackers");
    }
}

impl Bidder for OutbrainAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut req = request.clone();
        let mut errs = Vec::new();
        let mut outbrain_ext = ExtImpOutbrain::default();

        for imp in req.imp.iter_mut() {
            let bidder_ext = match imp.ext.as_ref()
                .and_then(|e| e.get("bidder"))
                .cloned()
            {
                Some(e) => e,
                None => continue,
            };
            let ext: ExtImpOutbrain = match serde_json::from_value(bidder_ext) {
                Ok(e) => e,
                Err(e) => {
                    errs.push(BidderError::BadInput(e.to_string()));
                    continue;
                }
            };
            if !ext.tag_id.is_empty() {
                imp.tagid = Some(ext.tag_id.clone());
            }
            outbrain_ext = ext;
        }

        // Set publisher on site or app
        let publisher = openrtb::Publisher {
            id: if outbrain_ext.publisher.id.is_empty() { None } else { Some(outbrain_ext.publisher.id.clone()) },
            name: if outbrain_ext.publisher.name.is_empty() { None } else { Some(outbrain_ext.publisher.name.clone()) },
            domain: if outbrain_ext.publisher.domain.is_empty() { None } else { Some(outbrain_ext.publisher.domain.clone()) },
            ..Default::default()
        };

        if let Some(site) = req.site.as_mut() {
            site.publisher = Some(publisher);
        } else if let Some(app) = req.app.as_mut() {
            app.publisher = Some(publisher);
        }

        if let Some(bcat) = outbrain_ext.bcat {
            req.bcat = Some(bcat);
        }
        if let Some(badv) = outbrain_ext.badv {
            req.badv = Some(badv);
        }

        let body = match serde_json::to_vec(&req) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        // Go adapter does not set any headers on the request
        let headers = HashMap::new();

        (vec![RequestData { method: "POST".to_string(), uri: self.endpoint.clone(), body, headers, imp_ids: get_imp_ids(&req.imp) }], errs)
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 {
            return Ok(BidderResponse::new());
        }
        if response.status_code == 400 {
            return Err(vec![BidderError::BadInput(
                "Unexpected status code: 400. Bad request from publisher. Run with request.debug = 1 for more info.".to_string()
            )]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info.",
                response.status_code
            ))]);
        }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(internal.imp.len());
        if let Some(cur) = bid_resp.cur {
            result.currency = cur;
        }
        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for mut bid in sb.bid {
                let bid_type = match get_media_type_for_imp(&bid.impid, &internal.imp) {
                    Ok(t) => t,
                    Err(e) => {
                        errs.push(e);
                        continue;
                    }
                };
                // For native bids, transform event trackers to imptrackers/jstracker
                if bid_type == BidType::Native {
                    if let Some(adm) = bid.adm.as_ref() {
                        match serde_json::from_str::<Value>(adm) {
                            Ok(mut native_payload) => {
                                transform_event_trackers(&mut native_payload);
                                match serde_json::to_string(&native_payload) {
                                    Ok(new_adm) => bid.adm = Some(new_adm),
                                    Err(e) => {
                                        errs.push(BidderError::BadServerResponse(e.to_string()));
                                        continue;
                                    }
                                }
                            }
                            Err(e) => {
                                errs.push(BidderError::BadServerResponse(e.to_string()));
                                continue;
                            }
                        }
                    }
                }
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        if !errs.is_empty() && result.bids.is_empty() {
            return Err(errs);
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
                banner: Some(openrtb::Banner { w: Some(300), h: Some(250), ..Default::default() }),
                ext: Some(serde_json::json!({"bidder": {"tagId": "tag1", "publisher": {"id": "pub1", "name": "Pubn"}}})),
                ..Default::default()
            }],
            site: Some(openrtb::Site::default()),
            ..Default::default()
        }
    }

    #[test]
    fn test_make_requests_basic() {
        let a = OutbrainAdapter::new("https://prebidtest.zemanta.com/api/bidder/prebidtest/bid/".to_string());
        let (reqs, errs) = a.make_requests(&make_req(), &ExtraRequestInfo::default());
        assert!(errs.is_empty());
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].uri, "https://prebidtest.zemanta.com/api/bidder/prebidtest/bid/");
        // Go adapter sets no headers
        assert!(reqs[0].headers.is_empty());
        let body: serde_json::Value = serde_json::from_slice(&reqs[0].body).unwrap();
        assert_eq!(body["imp"][0]["tagid"], "tag1");
        assert_eq!(body["site"]["publisher"]["id"], "pub1");
    }

    #[test]
    fn test_make_bids_basic() {
        let a = OutbrainAdapter::new("x".to_string());
        let body = br#"{"id":"r","seatbid":[{"bid":[{"id":"b1","impid":"i1","price":1.0}]}]}"#;
        let resp = ResponseData::new(200, body.to_vec());
        let result = a.make_bids(&make_req(), &RequestData::default(), &resp).unwrap();
        assert_eq!(result.bids.len(), 1);
        assert_eq!(result.bids[0].bid_type, BidType::Banner);
    }
}
