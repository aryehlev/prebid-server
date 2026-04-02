use std::collections::HashMap;
use pbs_adapters::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;

pub struct EplanningAdapter {
    pub endpoint: String,
}

impl EplanningAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

#[derive(Deserialize)]
struct HbResponse {
    #[serde(rename = "sp")]
    spaces: Vec<HbResponseSpace>,
}

#[derive(Deserialize)]
struct HbResponseSpace {
    #[serde(rename = "k")]
    name: String,
    #[serde(rename = "a")]
    ads: Vec<HbResponseAd>,
}

#[derive(Deserialize)]
struct HbResponseAd {
    #[serde(rename = "i")]
    impression_id: String,
    #[serde(rename = "id", default)]
    ad_id: String,
    #[serde(rename = "pr")]
    price: String,
    #[serde(rename = "adm")]
    adm: String,
    #[serde(rename = "crid")]
    cr_id: String,
    #[serde(rename = "adom", default)]
    adomain: String,
    #[serde(rename = "w", default)]
    width: u64,
    #[serde(rename = "h", default)]
    height: u64,
}

impl Bidder for EplanningAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        // EPlanning uses a custom GET-based protocol, not standard OpenRTB POST.
        // Build the URI based on client_id, target, spaces, and query params.
        let mut errs = Vec::new();

        let mut client_id = String::new();
        let mut spaces: Vec<String> = Vec::new();
        let mut vast_index = 0i32;
        let mut imp_type = 0i32; // 0=banner, 1=instream, 2=outstream

        // Determine request imp type (video instream/outstream/banner)
        for imp in &request.imp {
            if let Some(video) = &imp.video {
                let placement = video.placement.unwrap_or(0);
                if placement == 1 {
                    imp_type = 1; // instream
                } else if imp_type == 0 {
                    imp_type = 2; // outstream
                }
            }
        }

        for imp in &request.imp {
            let bidder = imp.ext.as_ref()
                .and_then(|e| e.get("bidder"));

            let c_id = bidder
                .and_then(|b| b.get("clientID").or_else(|| b.get("client_id")))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            if c_id.is_empty() {
                errs.push(BidderError::BadInput(format!("Ignoring imp id={}, no ClientID present", imp.id)));
                continue;
            }

            if client_id.is_empty() {
                client_id = c_id;
            }

            let ad_unit = bidder
                .and_then(|b| b.get("adUnitCode").or_else(|| b.get("ad_unit_code")))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let size_str = "1x1".to_string(); // simplified; full impl would compute from banner/video formats

            let name = if ad_unit.is_empty() { size_str.clone() } else { clean_name(&ad_unit) };

            if imp.video.is_some() {
                let vname = format!("video_{}_{}", size_str, vast_index);
                spaces.push(format!("{}:{};1", vname, size_str));
                vast_index += 1;
            } else {
                spaces.push(format!("{}:{}", name, size_str));
            }
        }

        if client_id.is_empty() {
            return (vec![], errs);
        }

        let page_url = request.site.as_ref()
            .and_then(|s| s.page.as_deref())
            .unwrap_or("FILE")
            .to_string();

        let page_domain = request.site.as_ref()
            .and_then(|s| s.domain.as_deref().or_else(|| s.page.as_deref()))
            .unwrap_or("FILE")
            .to_string();

        let request_target = request.app.as_ref()
            .and_then(|a| a.bundle.as_deref())
            .unwrap_or(&page_domain)
            .to_string();

        let spaces_str = spaces.join("+");

        let mut query_parts: Vec<String> = vec![
            "ncb=1".to_string(),
            format!("e={}", urlencoding_simple(&spaces_str)),
        ];

        if request.app.is_none() {
            query_parts.push(format!("ur={}", urlencoding_simple(&page_url)));
        }

        if let Some(user) = &request.user {
            if let Some(buyer_uid) = &user.buyeruid {
                if !buyer_uid.is_empty() {
                    query_parts.push(format!("uid={}", buyer_uid));
                }
            }
        }

        if imp_type > 0 {
            query_parts.push(format!("vctx={}", imp_type));
            query_parts.push("vv=3".to_string());
        }

        let uri = format!(
            "{}/{}/{}/{}/ROS?{}",
            self.endpoint.trim_end_matches('/'),
            client_id,
            "1",
            request_target,
            query_parts.join("&")
        );

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        if let Some(device) = &request.device {
            if let Some(ua) = &device.ua {
                headers.insert("User-Agent".to_string(), ua.clone());
            }
            if let Some(ip) = &device.ip {
                headers.insert("X-Forwarded-For".to_string(), ip.clone());
            }
        }

        (
            vec![RequestData {
                method: "GET".to_string(),
                uri,
                body: vec![],
                headers,
                imp_ids: get_imp_ids(&request.imp),
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
        if let Err(e) = pbs_adapters::check_response_status(response.status_code) {
            return Err(vec![e]);
        }

        let parsed: HbResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(format!(
                "Error unmarshaling HB response: {}", e
            ))])?;

        // Determine imp type for bid type assignment
        let imp_type = {
            let mut t = 0i32;
            for imp in &internal.imp {
                if let Some(video) = &imp.video {
                    let placement = video.placement.unwrap_or(0);
                    if placement == 1 {
                        t = 1;
                    } else if t == 0 {
                        t = 2;
                    }
                }
            }
            t
        };

        // Build space name -> imp id map
        let mut vast_index = 0i32;
        let mut space_to_imp: HashMap<String, String> = HashMap::new();
        for imp in &internal.imp {
            let bidder = imp.ext.as_ref().and_then(|e| e.get("bidder"));
            let ad_unit = bidder
                .and_then(|b| b.get("adUnitCode").or_else(|| b.get("ad_unit_code")))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let size_str = "1x1".to_string();
            let name = if ad_unit.is_empty() { size_str.clone() } else { clean_name(&ad_unit) };

            if imp.video.is_some() {
                let vname = format!("video_{}_{}", size_str, vast_index);
                space_to_imp.insert(vname, imp.id.clone());
                vast_index += 1;
            } else {
                space_to_imp.insert(name, imp.id.clone());
            }
        }

        let bid_type = if imp_type > 0 { BidType::Video } else { BidType::Banner };
        let mut result = BidderResponse::new();

        for space in parsed.spaces {
            let imp_id = space_to_imp.get(&space.name).cloned().unwrap_or_default();
            for ad in space.ads {
                if let Ok(price) = ad.price.parse::<f64>() {
                    let bid = openrtb::Bid {
                        id: ad.impression_id.clone(),
                        impid: imp_id.clone(),
                        price,
                        adm: Some(ad.adm),
                        crid: Some(ad.cr_id),
                        w: Some(ad.width as i32),
                        h: Some(ad.height as i32),
                        adid: if ad.ad_id.is_empty() { None } else { Some(ad.ad_id) },
                        adomain: if ad.adomain.is_empty() { None } else { Some(vec![ad.adomain]) },
                        ..Default::default()
                    };
                    result.bids.push(TypedBid::new(bid, bid_type.clone()));
                }
            }
        }

        Ok(result)
    }
}

fn clean_name(name: &str) -> String {
    // Replicate Go's cleanName: remove _.-/ and replace ()(): with _
    let mut s = name.to_string();
    s = s.replace(['_', '.', '-', '/'], "");
    s = s.replace(")(", "_").replace('(', "_").replace(')', "_").replace(':', "_");
    s = s.trim_matches('_').to_string();
    s
}

fn urlencoding_simple(s: &str) -> String {
    // Simple percent-encoding for query values
    s.chars().map(|c| match c {
        'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
        ' ' => "%20".to_string(),
        c => format!("%{:02X}", c as u32),
    }).collect()
}
