use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::{BidType, ExtBidPrebidVideo};
use serde::Deserialize;
use serde_json::Value;

const CLIENT_VERSION: &str = "prebid_server_1.2";

pub struct SmaatoAdapter { pub endpoint: String }
impl SmaatoAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

#[derive(Debug, Default, Deserialize)]
struct BidExt {
    #[serde(default)]
    duration: i32,
    #[serde(default)]
    curls: Vec<String>,
}

/// Split a single impression into one per media type (banner, video, native).
fn split_impressions_by_media_type(imp: &openrtb::Imp) -> Result<Vec<openrtb::Imp>, BidderError> {
    if imp.banner.is_none() && imp.video.is_none() && imp.native.is_none() {
        return Err(BidderError::BadInput(
            "Invalid MediaType. Smaato only supports Banner, Video and Native.".to_string(),
        ));
    }
    let mut imps = Vec::new();
    if imp.banner.is_some() {
        let mut c = imp.clone();
        c.video = None;
        c.native = None;
        imps.push(c);
    }
    if imp.video.is_some() {
        let mut c = imp.clone();
        c.banner = None;
        c.native = None;
        imps.push(c);
    }
    if imp.native.is_some() {
        let mut c = imp.clone();
        c.banner = None;
        c.video = None;
        imps.push(c);
    }
    Ok(imps)
}

/// Extract publisherId from imp.ext.bidder.publisherId
fn get_publisher_id(imp: &openrtb::Imp) -> Result<String, BidderError> {
    let pid = imp.ext.as_ref()
        .and_then(|e| e.get("bidder"))
        .and_then(|b| b.get("publisherId"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if pid.is_empty() {
        Err(BidderError::BadInput("Missing publisherId parameter.".to_string()))
    } else {
        Ok(pid.to_string())
    }
}

/// Extract adspaceId from imp.ext.bidder.adspaceId
fn get_adspace_id(imp: &openrtb::Imp) -> Result<String, BidderError> {
    let id = imp.ext.as_ref()
        .and_then(|e| e.get("bidder"))
        .and_then(|b| b.get("adspaceId"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if id.is_empty() {
        Err(BidderError::BadInput("Missing adspaceId parameter.".to_string()))
    } else {
        Ok(id.to_string())
    }
}

/// Remove the bidder node from imp.ext, returning None if ext becomes empty.
fn remove_bidder_from_imp_ext(ext: Option<Value>) -> Option<Value> {
    if let Some(Value::Object(mut map)) = ext {
        map.remove("bidder");
        if map.is_empty() {
            None
        } else {
            Some(Value::Object(map))
        }
    } else {
        ext
    }
}

/// Prepare a per-impression request: set publisherId in site/app and tagid from adspaceId.
fn prepare_individual_request(
    request: &mut openrtb::BidRequest,
    imp: &mut openrtb::Imp,
) -> Result<(), BidderError> {
    let publisher_id = get_publisher_id(imp)?;
    let adspace_id = get_adspace_id(imp)?;

    // Set publisher in site or app
    if let Some(site) = &request.site {
        let mut site = site.clone();
        site.publisher = Some(openrtb::Publisher {
            id: Some(publisher_id),
            ..Default::default()
        });
        request.site = Some(site);
    } else if let Some(app) = &request.app {
        let mut app = app.clone();
        app.publisher = Some(openrtb::Publisher {
            id: Some(publisher_id),
            ..Default::default()
        });
        request.app = Some(app);
    } else {
        return Err(BidderError::BadInput("Missing Site/App.".to_string()));
    }

    // Set tag ID from adspace ID and remove bidder ext
    imp.tagid = Some(adspace_id);
    imp.ext = remove_bidder_from_imp_ext(imp.ext.take());
    Ok(())
}

/// Extract user ext data (keywords, gender, yob) and move fields to user.
fn prepare_user(request: &mut openrtb::BidRequest) {
    if let Some(user) = &request.user {
        if let Some(ext) = &user.ext {
            if let Some(data) = ext.get("data") {
                let keywords = data.get("keywords").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let gender = data.get("gender").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let yob = data.get("yob").and_then(|v| v.as_i64()).unwrap_or(0);

                let mut user_copy = user.clone();
                if !keywords.is_empty() {
                    user_copy.keywords = Some(keywords);
                }
                if !gender.is_empty() {
                    user_copy.gender = Some(gender);
                }
                if yob != 0 {
                    user_copy.yob = Some(yob as i32);
                }
                // Remove data from ext
                if let Some(Value::Object(mut ext_map)) = user_copy.ext.take() {
                    ext_map.remove("data");
                    user_copy.ext = if ext_map.is_empty() {
                        None
                    } else {
                        Some(Value::Object(ext_map))
                    };
                }
                request.user = Some(user_copy);
            }
        }
    }
}

/// Extract site keywords from site.ext.data.keywords.
fn prepare_site(request: &mut openrtb::BidRequest) {
    if let Some(site) = &request.site {
        if let Some(ext) = &site.ext {
            let keywords = ext.get("data")
                .and_then(|d| d.get("keywords"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if !keywords.is_empty() {
                let mut site_copy = site.clone();
                site_copy.keywords = Some(keywords);
                site_copy.ext = None;
                request.site = Some(site_copy);
            }
        }
    }
}

/// Set request.ext with client version.
fn set_request_ext(request: &mut openrtb::BidRequest) {
    request.ext = Some(serde_json::json!({ "client": CLIENT_VERSION }));
}

/// Build the ad markup type to BidType mapping from X-Smt-Adtype header.
fn convert_ad_markup_type(ad_type: &str) -> Result<BidType, BidderError> {
    match ad_type {
        "Img" | "Richmedia" => Ok(BidType::Banner),
        "Video" => Ok(BidType::Video),
        "Native" => Ok(BidType::Native),
        other => Err(BidderError::BadServerResponse(format!("Unknown markup type {}.", other))),
    }
}

/// For banner: wrap adm with click tracker script if curls are present.
fn render_banner_adm(adm: &str, curls: &[String]) -> String {
    if curls.is_empty() {
        return adm.to_string();
    }
    let mut clicks = String::new();
    for curl in curls {
        // URL-encode the click tracker
        let encoded: String = curl.bytes().flat_map(|b| {
            match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9'
                | b'-' | b'_' | b'.' | b'~' => vec![b as char],
                b' ' => vec!['+'],
                other => format!("%{:02X}", other).chars().collect::<Vec<_>>(),
            }
        }).collect();
        clicks.push_str(&format!(
            "fetch(decodeURIComponent('{}'.replace(/\\+/g, ' ')), {{cache: 'no-cache'}});",
            encoded
        ));
    }
    format!(r#"<div style="cursor:pointer" onclick="{}">{}</div>"#, clicks, adm)
}

/// For native: extract the inner "native" field from the ad markup.
/// Go's extractAdmNative unmarshals the markup, extracts native.Native, and re-marshals it.
fn render_native_adm(adm: &str) -> Result<String, BidderError> {
    let parsed: Value = serde_json::from_str(adm)
        .map_err(|_| BidderError::BadServerResponse(format!("Invalid ad markup {}.", adm)))?;
    let native_val = parsed.get("native")
        .ok_or_else(|| BidderError::BadServerResponse(format!("Invalid ad markup {}.", adm)))?;
    serde_json::to_string(native_val)
        .map_err(|_| BidderError::BadServerResponse(format!("Invalid ad markup {}.", adm)))
}

impl Bidder for SmaatoAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::BadInput("No impressions in bid request.".to_string())]);
        }

        let mut base_request = request.clone();
        prepare_user(&mut base_request);
        prepare_site(&mut base_request);
        set_request_ext(&mut base_request);

        let mut requests = Vec::new();
        let mut errs = Vec::new();

        for imp in &request.imp {
            // Split impression by media type
            let split_imps = match split_impressions_by_media_type(imp) {
                Ok(imps) => imps,
                Err(e) => { errs.push(e); continue; }
            };

            for mut single_imp in split_imps {
                let mut req = base_request.clone();

                if let Err(e) = prepare_individual_request(&mut req, &mut single_imp) {
                    errs.push(e);
                    continue;
                }

                req.imp = vec![single_imp];

                let body = match serde_json::to_vec(&req) {
                    Ok(b) => b,
                    Err(e) => { errs.push(BidderError::BadInput(e.to_string())); continue; }
                };

                let mut headers = HashMap::new();
                headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
                headers.insert("Accept".to_string(), "application/json".to_string());

                requests.push(RequestData {
                    method: "POST".to_string(),
                    uri: self.endpoint.clone(),
                    body,
                    headers,
                    imp_ids: get_imp_ids(&req.imp),
                });
            }
        }

        (requests, errs)
    }

    fn make_bids(&self, _internal: &openrtb::BidRequest, _external: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info.",
                response.status_code
            ))]);
        }

        // X-Smt-Adtype header determines bid type
        let ad_type = response.headers.get("X-Smt-Adtype")
            .or_else(|| response.headers.get("x-smt-adtype"))
            .cloned()
            .unwrap_or_default();

        if ad_type.is_empty() {
            return Err(vec![BidderError::BadServerResponse("X-Smt-Adtype header is missing.".to_string())]);
        }

        let bid_type = convert_ad_markup_type(&ad_type)
            .map_err(|e| vec![e])?;

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(5);
        let mut errs = Vec::new();

        for sb in bid_resp.seatbid {
            for mut bid in sb.bid {
                // Extract bid ext for duration and click trackers
                let bid_ext = bid.ext.as_ref()
                    .and_then(|e| serde_json::from_value::<BidExt>(e.clone()).ok())
                    .unwrap_or_default();

                // Render ad markup based on type
                let new_adm = match ad_type.as_str() {
                    "Img" | "Richmedia" => {
                        let adm = bid.adm.as_deref().unwrap_or("");
                        render_banner_adm(adm, &bid_ext.curls)
                    },
                    "Video" => bid.adm.clone().unwrap_or_default(),
                    "Native" => {
                        let adm = bid.adm.as_deref().unwrap_or("");
                        match render_native_adm(adm) {
                            Ok(s) => s,
                            Err(e) => { errs.push(e); continue; }
                        }
                    },
                    other => {
                        errs.push(BidderError::BadServerResponse(format!("Unknown markup type {}.", other)));
                        continue;
                    }
                };
                bid.adm = Some(new_adm);

                // Build video metadata
                let bid_video = if bid_type == BidType::Video {
                    let primary_category = bid.cat.as_ref()
                        .and_then(|c| c.first())
                        .cloned()
                        .unwrap_or_default();
                    Some(ExtBidPrebidVideo {
                        duration: bid_ext.duration,
                        primary_category,
                    })
                } else {
                    None
                };

                let mut typed_bid = TypedBid::new(bid, bid_type.clone());
                typed_bid.bid_video = bid_video;
                result.bids.push(typed_bid);
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
                banner: Some(openrtb::Banner {
                    w: Some(300),
                    h: Some(250),
                    ..Default::default()
                }),
                ext: Some(serde_json::json!({
                    "bidder": {"publisherId": "publisher1", "adspaceId": "adspace1"}
                })),
                ..Default::default()
            }],
            site: Some(openrtb::Site {
                id: Some("s".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn test_smaato_url_and_headers() {
        let adapter = SmaatoAdapter::new("https://prebid.ad.smaato.net/oapi/prebid".to_string());
        let (reqs, errs) = adapter.make_requests(&make_req(), &ExtraRequestInfo::default());
        assert!(errs.is_empty(), "errs: {:?}", errs);
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].uri, "https://prebid.ad.smaato.net/oapi/prebid");
        assert_eq!(
            reqs[0].headers.get("Content-Type").unwrap(),
            "application/json;charset=utf-8"
        );
        assert!(!reqs[0].body.is_empty());
    }

    #[test]
    fn test_smaato_make_bids() {
        let adapter = SmaatoAdapter::new("https://prebid.ad.smaato.net/oapi/prebid".to_string());
        let body = br#"{"id":"r","seatbid":[{"bid":[{"id":"b","impid":"i1","price":1.6,"crid":"c","adm":"<img/>"}]}]}"#;
        let mut resp = ResponseData::new(200, body.to_vec());
        resp.headers.insert("X-Smt-Adtype".to_string(), "Img".to_string());
        let result = adapter
            .make_bids(&make_req(), &RequestData::default(), &resp)
            .unwrap();
        assert_eq!(result.bids.len(), 1);
        assert_eq!(result.bids[0].bid_type, BidType::Banner);
    }
}
