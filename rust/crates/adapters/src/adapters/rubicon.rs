use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::{Deserialize, Serialize};

const BADV_LIMIT: usize = 50;

pub struct RubiconAdapter {
    pub endpoint: String,
    pub xapi_username: String,
    pub xapi_password: String,
}

impl RubiconAdapter {
    pub fn new(endpoint: String, xapi_username: String, xapi_password: String) -> Self {
        Self {
            endpoint,
            xapi_username,
            xapi_password,
        }
    }
}

/// Simple base64 encoding without external crate dependency.
fn base64_encode(input: &str) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = input.as_bytes();
    let mut result = String::new();
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let combined = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[((combined >> 18) & 63) as usize] as char);
        result.push(CHARS[((combined >> 12) & 63) as usize] as char);
        result.push(if chunk.len() > 1 {
            CHARS[((combined >> 6) & 63) as usize] as char
        } else {
            '='
        });
        result.push(if chunk.len() > 2 {
            CHARS[(combined & 63) as usize] as char
        } else {
            '='
        });
    }
    result
}

fn basic_auth(user: &str, pass: &str) -> String {
    let credentials = format!("{}:{}", user, pass);
    format!("Basic {}", base64_encode(&credentials))
}

/// Rubicon bidder extension from request imp
#[derive(Debug, Default, Deserialize)]
struct ExtImpRubicon {
    #[serde(rename = "accountId", default)]
    account_id: i64,
    #[serde(rename = "siteId", default)]
    site_id: i64,
    #[serde(rename = "zoneId", default)]
    zone_id: i64,
    #[serde(rename = "video", default)]
    video: ExtImpRubiconVideo,
    #[serde(rename = "inventory", default)]
    inventory: Option<serde_json::Value>,
    #[serde(rename = "visitor", default)]
    visitor: Option<serde_json::Value>,
    #[serde(rename = "keywords", default)]
    keywords: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ExtImpRubiconVideo {
    #[serde(rename = "skip", default)]
    skip: i32,
    #[serde(rename = "skipdelay", default)]
    skip_delay: i32,
    #[serde(rename = "videoSizeID", default)]
    video_size_id: i32,
}

/// Imp ext wrapper with bidder and optional fields
#[derive(Debug, Default, Deserialize)]
struct RubiconImpBidderExt {
    #[serde(rename = "bidder", default)]
    bidder: ExtImpRubicon,
    #[serde(rename = "gpid", default)]
    gpid: String,
    #[serde(rename = "tid", default)]
    tid: String,
}

/// Outgoing imp.ext for rubicon
#[derive(Serialize)]
struct RubiconImpExt {
    rp: RubiconImpExtRP,
    #[serde(skip_serializing_if = "String::is_empty")]
    gpid: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    tid: String,
}

#[derive(Serialize)]
struct RubiconImpExtRP {
    zone_id: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    target: Option<serde_json::Value>,
    track: RubiconImpExtRPTrack,
}

#[derive(Serialize)]
struct RubiconImpExtRPTrack {
    mint: String,
    mint_version: String,
}

/// Outgoing site.ext for rubicon
#[derive(Serialize)]
struct RubiconSiteExt {
    rp: RubiconSiteExtRP,
}

#[derive(Serialize)]
struct RubiconSiteExtRP {
    site_id: i64,
}

/// Outgoing publisher.ext for rubicon
#[derive(Serialize)]
struct RubiconPubExt {
    rp: RubiconPubExtRP,
}

#[derive(Serialize)]
struct RubiconPubExtRP {
    account_id: i64,
}

/// Outgoing video.ext for rubicon
#[derive(Serialize)]
struct RubiconVideoExt {
    #[serde(skip_serializing_if = "zero_i32")]
    skip: i32,
    #[serde(skip_serializing_if = "zero_i32")]
    skipdelay: i32,
    #[serde(skip_serializing_if = "String::is_empty")]
    videotype: String,
    rp: RubiconVideoExtRP,
}

fn zero_i32(v: &i32) -> bool { *v == 0 }

#[derive(Serialize)]
struct RubiconVideoExtRP {
    #[serde(skip_serializing_if = "zero_i32")]
    size_id: i32,
}

/// Outgoing banner.ext for rubicon - always `{"rp":{"mime":"text/html"}}`
#[derive(Serialize)]
struct RubiconBannerExt {
    rp: RubiconBannerExtRP,
}

#[derive(Serialize)]
struct RubiconBannerExtRP {
    mime: String,
}

fn is_video(imp: &openrtb::Imp) -> bool {
    if let Some(video) = imp.video.as_ref() {
        // Video takes priority if banner is absent or video is fully populated
        if imp.banner.is_none() {
            return true;
        }
        // fully populated video: mimes, protocols, max_duration, linearity all set
        let fully_populated = video.mimes.as_ref().map_or(false, |m| !m.is_empty())
            && video.protocols.as_ref().map_or(false, |p| !p.is_empty())
            && video.maxduration.unwrap_or(0) != 0
            && video.linearity.unwrap_or(0) != 0;
        return fully_populated;
    }
    false
}

impl Bidder for RubiconAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut request_data = Vec::new();

        let mut headers = HashMap::new();
        headers.insert(
            "Content-Type".to_string(),
            "application/json;charset=utf-8".to_string(),
        );
        headers.insert("Accept".to_string(), "application/json".to_string());
        headers.insert("User-Agent".to_string(), "prebid-server/1.0".to_string());
        headers.insert(
            "Authorization".to_string(),
            basic_auth(&self.xapi_username, &self.xapi_password),
        );

        for imp in &request.imp {
            let bidder_ext: RubiconImpBidderExt = match imp
                .ext
                .as_ref()
                .and_then(|e| serde_json::from_value(e.clone()).ok())
            {
                Some(e) => e,
                None => {
                    errs.push(BidderError::BadInput(format!(
                        "failed to parse rubicon imp ext for imp id={}",
                        imp.id
                    )));
                    continue;
                }
            };

            let rubicon_ext = &bidder_ext.bidder;

            // Build outgoing imp ext
            let imp_ext = RubiconImpExt {
                rp: RubiconImpExtRP {
                    zone_id: rubicon_ext.zone_id,
                    target: rubicon_ext.inventory.clone(),
                    track: RubiconImpExtRPTrack {
                        mint: String::new(),
                        mint_version: String::new(),
                    },
                },
                gpid: bidder_ext.gpid.clone(),
                tid: bidder_ext.tid.clone(),
            };

            let mut imp_copy = imp.clone();

            // Set secure=1
            imp_copy.secure = Some(1);

            // Set the imp ext
            match serde_json::to_value(&imp_ext) {
                Ok(v) => imp_copy.ext = Some(v),
                Err(e) => {
                    errs.push(BidderError::BadInput(e.to_string()));
                    continue;
                }
            }

            // Determine bid type and set appropriate media type fields
            let bid_type;
            if is_video(&imp_copy) {
                bid_type = BidType::Video;

                let video_copy = imp_copy.video.clone().unwrap_or_default();
                let video_type = if imp_copy.rwdd.unwrap_or(0) == 1 {
                    imp_copy.rwdd = Some(0);
                    "rewarded".to_string()
                } else {
                    String::new()
                };

                let video_ext = RubiconVideoExt {
                    skip: rubicon_ext.video.skip,
                    skipdelay: rubicon_ext.video.skip_delay,
                    videotype: video_type,
                    rp: RubiconVideoExtRP {
                        size_id: rubicon_ext.video.video_size_id,
                    },
                };
                let mut vc = video_copy;
                vc.ext = serde_json::to_value(&video_ext).ok();
                imp_copy.video = Some(vc);
                imp_copy.banner = None;
                imp_copy.native = None;
            } else if imp_copy.banner.is_some() {
                bid_type = BidType::Banner;

                let banner_ext = RubiconBannerExt {
                    rp: RubiconBannerExtRP {
                        mime: "text/html".to_string(),
                    },
                };
                let mut bc = imp_copy.banner.clone().unwrap_or_default();
                // Validate banner has format or dimensions
                let has_format = bc.format.as_ref().map_or(false, |f| !f.is_empty());
                if !has_format
                    && (bc.w.is_none() || bc.w == Some(0))
                    && (bc.h.is_none() || bc.h == Some(0))
                {
                    errs.push(BidderError::BadInput(
                        "rubicon imps must have at least one imp.format element".to_string(),
                    ));
                    continue;
                }
                bc.ext = serde_json::to_value(&banner_ext).ok();
                imp_copy.banner = Some(bc);
                imp_copy.video = None;
                imp_copy.native = None;
            } else if imp_copy.native.is_some() {
                bid_type = BidType::Native;
                imp_copy.video = None;
            } else {
                errs.push(BidderError::BadInput(format!(
                    "no supported media type for imp id={}",
                    imp.id
                )));
                continue;
            }

            // Build site/app publisher ext
            let pub_ext = RubiconPubExt {
                rp: RubiconPubExtRP {
                    account_id: rubicon_ext.account_id,
                },
            };
            let pub_ext_val = match serde_json::to_value(&pub_ext) {
                Ok(v) => v,
                Err(e) => {
                    errs.push(BidderError::BadInput(e.to_string()));
                    continue;
                }
            };

            let site_ext = RubiconSiteExt {
                rp: RubiconSiteExtRP {
                    site_id: rubicon_ext.site_id,
                },
            };
            let site_ext_val = match serde_json::to_value(&site_ext) {
                Ok(v) => v,
                Err(e) => {
                    errs.push(BidderError::BadInput(e.to_string()));
                    continue;
                }
            };

            let mut rubicon_request = request.clone();
            rubicon_request.imp = vec![imp_copy];
            rubicon_request.cur = None;
            rubicon_request.ext = None;

            // Limit badv
            if let Some(ref badv) = rubicon_request.badv {
                if badv.len() > BADV_LIMIT {
                    rubicon_request.badv = Some(badv[..BADV_LIMIT].to_vec());
                }
            }

            // Set site or app publisher/ext
            if let Some(site) = rubicon_request.site.as_mut() {
                site.ext = Some(site_ext_val);
                let mut pub_obj = site.publisher.clone().unwrap_or_default();
                pub_obj.ext = Some(pub_ext_val.clone());
                site.publisher = Some(pub_obj);
            } else if let Some(app) = rubicon_request.app.as_mut() {
                app.ext = Some(site_ext_val);
                let mut pub_obj = app.publisher.clone().unwrap_or_default();
                pub_obj.ext = Some(pub_ext_val.clone());
                app.publisher = Some(pub_obj);
            }

            let body = match serde_json::to_vec(&rubicon_request) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(BidderError::BadInput(e.to_string()));
                    continue;
                }
            };

            let imp_ids = get_imp_ids(&rubicon_request.imp);

            request_data.push(RequestData {
                method: "POST".to_string(),
                uri: self.endpoint.clone(),
                body,
                headers: headers.clone(),
                imp_ids,
            });

            let _ = bid_type; // bid type is determined in make_bids from external request
        }

        (request_data, errs)
    }

    fn make_bids(
        &self,
        _internal: &openrtb::BidRequest,
        external: &RequestData,
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
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        // Parse the external (outgoing) request to determine bid type per imp
        let ext_req: openrtb::BidRequest = serde_json::from_slice(&external.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let bid_type = if let Some(imp) = ext_req.imp.first() {
            if is_video(imp) {
                BidType::Video
            } else if imp.banner.is_some() {
                BidType::Banner
            } else {
                BidType::Native
            }
        } else {
            BidType::Banner
        };

        let mut result = BidderResponse::with_capacity(5);

        if let Some(cur) = bid_resp.cur.as_deref() {
            if !cur.is_empty() {
                result.currency = cur.to_string();
            }
        }

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                // Only include bids with non-zero price (Go skips zero-price bids)
                if bid.price != 0.0 {
                    result.bids.push(TypedBid::new(bid, bid_type.clone()));
                }
            }
        }

        Ok(result)
    }
}
