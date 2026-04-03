use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::{Deserialize, Serialize};

pub struct DmxAdapter { pub endpoint: String }
impl DmxAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
struct DmxParams {
    #[serde(rename = "tagid", default)]
    tag_id: String,
    #[serde(rename = "dmxid", default)]
    dmx_id: String,
    #[serde(rename = "memberid", default)]
    member_id: String,
    #[serde(rename = "publisher_id", default)]
    publisher_id: String,
    #[serde(rename = "seller_id", default)]
    seller_id: String,
    #[serde(rename = "bidfloor", default)]
    bidfloor: f64,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct DmxExt {
    #[serde(rename = "bidder")]
    bidder: DmxParams,
}

fn user_seller_or_pub_id(s1: &str, s2: &str) -> String {
    if !s1.is_empty() { s1.to_string() } else { s2.to_string() }
}

fn get_media_type_for_imp(imp_id: &str, imps: &[openrtb::Imp]) -> Result<BidType, BidderError> {
    for imp in imps {
        if imp.id == imp_id {
            let media_type = if imp.banner.is_none() && imp.video.is_some() {
                BidType::Video
            } else {
                BidType::Banner
            };
            return Ok(media_type);
        }
    }
    Err(BidderError::BadInput(format!("Failed to find impression \"{}\" ", imp_id)))
}

fn video_imp_insertion(adm: &str, nurl: &str) -> String {
    let search = "</Impression>";
    let wrapped = format!("</Impression><Impression><![CDATA[{}]]></Impression>", nurl);
    adm.replacen(search, &wrapped, 1)
}

fn check_protocols(protocols: &Option<Vec<i32>>) -> Vec<i32> {
    if let Some(p) = protocols {
        if !p.is_empty() {
            return p.clone();
        }
    }
    // default protocols: 2, 3, 5, 6, 7, 8
    vec![2, 3, 5, 6, 7, 8]
}

impl Bidder for DmxAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();

        // Require user or app
        if request.user.is_none() && request.app.is_none() {
            return (vec![], vec![BidderError::BadInput("No user id or app id found. Could not send request to DMX.".to_string())]);
        }

        // Parse first imp ext for publisherId and sellerId
        let mut publisher_id = String::new();
        let mut seller_id = String::new();
        let mut root_ext = DmxParams::default();

        if !request.imp.is_empty() {
            if let Some(ext) = &request.imp[0].ext {
                if let Some(bidder_val) = ext.get("bidder") {
                    match serde_json::from_value::<DmxParams>(bidder_val.clone()) {
                        Ok(p) => {
                            publisher_id = user_seller_or_pub_id(&p.publisher_id, &p.member_id);
                            seller_id = p.seller_id.clone();
                            root_ext = p;
                        }
                        Err(e) => errs.push(BidderError::BadInput(e.to_string())),
                    }
                }
            }
        }

        let mut dmx_req = request.clone();

        // Handle app
        if let Some(app) = &request.app {
            let mut app_copy = app.clone();
            if let Some(pub_ref) = &app.publisher {
                let mut pub_copy = pub_ref.clone();
                if pub_copy.id.as_deref().unwrap_or("").is_empty() {
                    pub_copy.id = Some(publisher_id.clone());
                }
                let dmx_pub_id = user_seller_or_pub_id(&root_ext.publisher_id, &root_ext.member_id);
                let ext_val = serde_json::json!({ "dmx": { "id": dmx_pub_id } });
                pub_copy.ext = Some(ext_val);
                app_copy.publisher = Some(pub_copy);
            }
            dmx_req.app = Some(app_copy);
        } else {
            dmx_req.app = None;
        }

        // Handle site
        if let Some(site) = &request.site {
            let mut site_copy = site.clone();
            if let Some(pub_ref) = &site.publisher {
                let mut pub_copy = pub_ref.clone();
                if pub_copy.id.as_deref().unwrap_or("").is_empty() {
                    pub_copy.id = Some(publisher_id.clone());
                }
                let dmx_pub_id = user_seller_or_pub_id(&root_ext.publisher_id, &root_ext.member_id);
                let ext_val = serde_json::json!({ "dmx": { "id": dmx_pub_id } });
                pub_copy.ext = Some(ext_val);
                site_copy.publisher = Some(pub_copy);
            } else {
                site_copy.publisher = Some(openrtb::Publisher {
                    id: Some(publisher_id.clone()),
                    ..Default::default()
                });
            }
            dmx_req.site = Some(site_copy);
        } else {
            dmx_req.site = None;
        }

        // Handle user
        if let Some(user) = &request.user {
            dmx_req.user = Some(user.clone());
        } else {
            dmx_req.user = None;
        }

        // Check hasNoID
        let mut has_no_id = true;

        if request.app.is_some() {
            if let Some(app) = &request.app {
                if app.id.as_deref().unwrap_or("").is_empty() {
                    // check device IFA
                    if let Some(device) = &request.device {
                        if !device.ifa.as_deref().unwrap_or("").is_empty() {
                            if let Some(app_mut) = dmx_req.app.as_mut() {
                                app_mut.id = device.ifa.clone();
                            }
                            has_no_id = false;
                        }
                    }
                } else {
                    has_no_id = false;
                }
            }
        }

        if let Some(user) = &dmx_req.user {
            if !user.id.as_deref().unwrap_or("").is_empty() {
                has_no_id = false;
            }
            // Check eids in user ext
            if let Some(ext) = &user.ext {
                if let Some(eids) = ext.get("eids") {
                    if eids.as_array().map(|a| !a.is_empty()).unwrap_or(false) {
                        has_no_id = false;
                    }
                }
            }
        }

        // Build impressions
        let mut imps = Vec::new();
        for inst in &request.imp {
            let params: DmxParams = if let Some(ext) = &inst.ext {
                if let Some(bidder_val) = ext.get("bidder") {
                    match serde_json::from_value::<DmxParams>(bidder_val.clone()) {
                        Ok(p) => p,
                        Err(e) => {
                            errs.push(BidderError::BadInput(e.to_string()));
                            continue;
                        }
                    }
                } else {
                    continue;
                }
            } else {
                continue;
            };

            let has_pub = !params.publisher_id.is_empty() || !params.member_id.is_empty();
            const SECURE: i32 = 1;

            if let Some(banner) = &inst.banner {
                if !banner.format.as_deref().unwrap_or(&[]).is_empty() {
                    if !has_pub {
                        return (vec![], vec![BidderError::BadInput("Missing Params for auction to be send".to_string())]);
                    }
                    let mut temp_imp = inst.clone();
                    if params.bidfloor != 0.0 {
                        temp_imp.bidfloor = Some(params.bidfloor);
                    }
                    if !params.tag_id.is_empty() {
                        temp_imp.tagid = Some(params.tag_id.clone());
                        temp_imp.secure = Some(SECURE);
                    }
                    if !params.dmx_id.is_empty() {
                        temp_imp.tagid = Some(params.dmx_id.clone());
                        temp_imp.secure = Some(SECURE);
                    }
                    if temp_imp.tagid.as_deref().unwrap_or("").is_empty() {
                        continue;
                    }
                    let mut banner_copy = banner.clone();
                    if banner_copy.w.is_none() || banner_copy.h.is_none() {
                        if let Some(fmt) = banner_copy.format.as_deref().and_then(|f| f.first()) {
                            banner_copy.w = fmt.w;
                            banner_copy.h = fmt.h;
                        }
                    }
                    temp_imp.banner = Some(banner_copy);
                    temp_imp.video = None;
                    imps.push(temp_imp);
                }
            }

            if let Some(video) = &inst.video {
                if !has_pub {
                    return (vec![], vec![BidderError::BadInput("Missing Params for auction to be send".to_string())]);
                }
                let mut temp_imp = inst.clone();
                if params.bidfloor != 0.0 {
                    temp_imp.bidfloor = Some(params.bidfloor);
                }
                if !params.tag_id.is_empty() {
                    temp_imp.tagid = Some(params.tag_id.clone());
                    temp_imp.secure = Some(SECURE);
                }
                if !params.dmx_id.is_empty() {
                    temp_imp.tagid = Some(params.dmx_id.clone());
                    temp_imp.secure = Some(SECURE);
                }
                if temp_imp.tagid.as_deref().unwrap_or("").is_empty() {
                    continue;
                }
                let mut video_copy = video.clone();
                video_copy.protocols = Some(check_protocols(&video.protocols));
                temp_imp.video = Some(video_copy);
                temp_imp.banner = None;
                imps.push(temp_imp);
            }
        }

        dmx_req.imp = imps;

        if has_no_id {
            return (vec![], vec![BidderError::BadInput("This request contained no identifier".to_string())]);
        }

        let body = match serde_json::to_vec(&dmx_req) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let uri = if !seller_id.is_empty() {
            let encoded = url_encode(&seller_id);
            format!("{}?sellerid={}", self.endpoint, encoded)
        } else {
            self.endpoint.clone()
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());

        let imp_ids = get_imp_ids(&dmx_req.imp);
        (vec![RequestData { method: "POST".to_string(), uri, body, headers, imp_ids }], errs)
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if response.status_code == 400 {
            return Err(vec![BidderError::BadInput("Unexpected status code 400".to_string())]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadInput("Unexpected response no status code".to_string())]);
        }
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;
        let mut result = BidderResponse::with_capacity(5);
        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for mut bid in sb.bid {
                match get_media_type_for_imp(&bid.impid, &internal.imp) {
                    Ok(bid_type) => {
                        if bid_type == BidType::Video {
                            let adm = bid.adm.as_deref().unwrap_or("").to_string();
                            let nurl = bid.nurl.as_deref().unwrap_or("").to_string();
                            bid.adm = Some(video_imp_insertion(&adm, &nurl));
                        }
                        result.bids.push(TypedBid::new(bid, bid_type));
                    }
                    Err(e) => errs.push(e),
                }
            }
        }
        Ok(result)
    }
}

fn url_encode(s: &str) -> String {
    s.chars().map(|c| match c {
        'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
        ' ' => "+".to_string(),
        _ => format!("%{:02X}", c as u32),
    }).collect()
}
