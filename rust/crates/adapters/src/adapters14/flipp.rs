use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use openrtb_ext::BidType;
use serde::{Deserialize, Serialize};

pub struct FlippAdapter { pub endpoint: String }
impl FlippAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

const DEFAULT_CURRENCY: &str = "USD";
const INLINE_DIV_NAME: &str = "inline";
const DEFAULT_STANDARD_HEIGHT: i64 = 2400;
const DEFAULT_COMPACT_HEIGHT: i64 = 600;

static AD_TYPES: &[i64] = &[4309, 641];
static DTX_TYPES: &[i64] = &[5061];

#[derive(Debug, Deserialize, Clone, Default)]
struct ImpExtFlipp {
    #[serde(rename = "publisherNameIdentifier", default)]
    publisher_name_identifier: String,
    #[serde(rename = "creativeType", default)]
    creative_type: String,
    #[serde(rename = "siteId", default)]
    site_id: i64,
    #[serde(rename = "zoneIds", default)]
    zone_ids: Vec<i64>,
    #[serde(rename = "userKey", default)]
    user_key: String,
    #[serde(default)]
    options: ImpExtFlippOptions,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
struct ImpExtFlippOptions {
    #[serde(rename = "startCompact", default)]
    start_compact: bool,
    #[serde(rename = "contentCode", default)]
    content_code: String,
}

#[derive(Debug, Serialize, Clone)]
struct CampaignRequestBody {
    ip: String,
    keywords: Vec<String>,
    placements: Vec<Placement>,
    url: String,
    user: CampaignRequestBodyUser,
}

#[derive(Debug, Serialize, Clone)]
struct CampaignRequestBodyUser {
    key: String,
}

#[derive(Debug, Serialize, Clone)]
struct Placement {
    #[serde(rename = "adTypes")]
    ad_types: Vec<i64>,
    count: i64,
    #[serde(rename = "divName")]
    div_name: String,
    prebid: PrebidRequest,
    properties: Properties,
    #[serde(rename = "siteId")]
    site_id: i64,
    #[serde(rename = "zoneIds")]
    zone_ids: Vec<i64>,
    options: serde_json::Value,
}

#[derive(Debug, Serialize, Clone)]
struct Properties {
    #[serde(rename = "contentCode", skip_serializing_if = "Option::is_none")]
    content_code: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
struct PrebidRequest {
    #[serde(rename = "creativeType")]
    creative_type: String,
    height: i64,
    #[serde(rename = "publisherNameIdentifier")]
    publisher_name_identifier: String,
    #[serde(rename = "requestId")]
    request_id: String,
    width: i64,
}

#[derive(Debug, Deserialize, Clone)]
struct CampaignResponseBody {
    decisions: Option<Decisions>,
}

#[derive(Debug, Deserialize, Clone)]
struct Decisions {
    inline: Option<Vec<InlineModel>>,
}

#[derive(Debug, Deserialize, Clone)]
struct InlineModel {
    #[serde(rename = "adId", default)]
    ad_id: i64,
    #[serde(rename = "creativeId", default)]
    creative_id: i64,
    contents: Option<Vec<Content>>,
    prebid: Option<PrebidResponse>,
}

#[derive(Debug, Deserialize, Clone)]
struct Content {
    data: Option<ContentData>,
}

#[derive(Debug, Deserialize, Clone)]
struct ContentData {
    width: Option<i64>,
    #[serde(rename = "customData")]
    custom_data: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Clone)]
struct PrebidResponse {
    cpm: Option<f64>,
    creative: Option<String>,
    #[serde(rename = "requestId")]
    request_id: Option<String>,
}

fn get_ad_types(creative_type: &str) -> Vec<i64> {
    if creative_type == "DTX" {
        DTX_TYPES.to_vec()
    } else {
        AD_TYPES.to_vec()
    }
}

impl Bidder for FlippAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = Vec::new();
        let mut errs = Vec::new();

        let site_page = request.site.as_ref().map(|s| s.page.as_deref().unwrap_or("")).unwrap_or("");
        let site_keywords = request.site.as_ref().map(|s| s.keywords.as_deref().unwrap_or("")).unwrap_or("");

        let user_ip = request.device.as_ref().and_then(|d| {
            d.ip.as_ref().filter(|s| !s.is_empty()).cloned()
        });

        if user_ip.is_none() {
            errs.push(BidderError::BadInput("no IP set in flipp bidder params or request device".to_string()));
            return (vec![], errs);
        }

        let user_key = request.user.as_ref()
            .and_then(|u| u.id.as_ref().filter(|s| !s.is_empty()).cloned());

        for imp in &request.imp {
            let bidder_val = match imp.ext.as_ref().and_then(|e| e.get("bidder")) {
                Some(v) => v.clone(),
                None => {
                    errs.push(BidderError::BadInput("flipp params not found.".to_string()));
                    continue;
                }
            };
            let flipp_params: ImpExtFlipp = match serde_json::from_value(bidder_val) {
                Ok(p) => p,
                Err(e) => {
                    errs.push(BidderError::BadInput(format!("unable to extract flipp params. {}", e)));
                    continue;
                }
            };

            // Determine content code
            let content_code = if !flipp_params.options.content_code.is_empty() {
                Some(flipp_params.options.content_code.clone())
            } else {
                // Try to get from site URL query param
                let cc = site_page.split('?').nth(1)
                    .and_then(|q| q.split('&').find(|p| p.starts_with("flipp-content-code=")))
                    .and_then(|p| p.strip_prefix("flipp-content-code="))
                    .map(|s| s.to_string());
                cc
            };

            // Banner dimensions
            let (height, width) = imp.banner.as_ref()
                .and_then(|b| b.format.as_ref())
                .and_then(|f| f.first())
                .map(|f| (f.h.unwrap_or(0) as i64, f.w.unwrap_or(0) as i64))
                .unwrap_or((0, 0));

            let placement = Placement {
                ad_types: get_ad_types(&flipp_params.creative_type),
                count: 1,
                div_name: INLINE_DIV_NAME.to_string(),
                prebid: PrebidRequest {
                    creative_type: flipp_params.creative_type.clone(),
                    height,
                    publisher_name_identifier: flipp_params.publisher_name_identifier.clone(),
                    request_id: imp.id.clone(),
                    width,
                },
                properties: Properties {
                    content_code: content_code.filter(|s| !s.is_empty()),
                },
                site_id: flipp_params.site_id,
                zone_ids: flipp_params.zone_ids.clone(),
                options: serde_json::to_value(&flipp_params.options).unwrap_or(serde_json::Value::Null),
            };

            // User key: prefer user.id, then params userKey, else generate uuid-like
            let uk = user_key.clone()
                .or_else(|| if !flipp_params.user_key.is_empty() { Some(flipp_params.user_key.clone()) } else { None })
                .unwrap_or_else(|| format!("uid-{}", imp.id));

            let keywords: Vec<String> = site_keywords.split(',').map(|s| s.to_string()).collect();

            let campaign_body = CampaignRequestBody {
                placements: vec![placement],
                url: site_page.to_string(),
                keywords,
                ip: user_ip.clone().unwrap_or_default(),
                user: CampaignRequestBodyUser { key: uk },
            };

            let body = match serde_json::to_vec(&campaign_body) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(BidderError::BadInput(format!("make request failed with err {}", e)));
                    continue;
                }
            };

            let mut headers = HashMap::new();
            headers.insert("Content-Type".to_string(), "application/json".to_string());
            if let Some(device) = &request.device {
                if let Some(ua) = &device.ua { if !ua.is_empty() {
                    headers.insert("User-Agent".to_string(), ua.clone());
                }}
            }

            requests.push(RequestData {
                method: "POST".to_string(),
                uri: self.endpoint.clone(),
                body,
                headers,
                imp_ids: vec![imp.id.clone()],
            });
        }

        if requests.is_empty() {
            errs.push(BidderError::BadInput("adapterRequest is empty".to_string()));
        }

        (requests, errs)
    }

    fn make_bids(&self, request: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
        if let Err(e) = crate::check_response_status(response.status_code) { return Err(vec![e]); }

        let campaign_resp: CampaignResponseBody = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(request.imp.len());
        result.currency = DEFAULT_CURRENCY.to_string();

        let inline_decisions = campaign_resp.decisions
            .as_ref()
            .and_then(|d| d.inline.as_ref())
            .map(|v| v.as_slice())
            .unwrap_or(&[]);

        for imp in &request.imp {
            // Parse flipp params for this imp
            let flipp_params: ImpExtFlipp = imp.ext.as_ref()
                .and_then(|e| e.get("bidder"))
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();

            for decision in inline_decisions {
                let req_id = decision.prebid.as_ref()
                    .and_then(|p| p.request_id.as_deref())
                    .unwrap_or("");

                if req_id != imp.id {
                    continue;
                }

                let cpm = decision.prebid.as_ref().and_then(|p| p.cpm).unwrap_or(0.0);
                let creative = decision.prebid.as_ref().and_then(|p| p.creative.clone()).unwrap_or_default();

                let mut w: i64 = 0;
                let mut h: i64 = if flipp_params.options.start_compact { DEFAULT_COMPACT_HEIGHT } else { DEFAULT_STANDARD_HEIGHT };

                if let Some(contents) = &decision.contents {
                    if let Some(first) = contents.first() {
                        if let Some(data) = &first.data {
                            if let Some(dw) = data.width {
                                w = dw;
                            }
                            // Try to get height from customData
                            if let Some(cd) = &data.custom_data {
                                let key = if flipp_params.options.start_compact { "compactHeight" } else { "standardHeight" };
                                if let Some(val) = cd.get(key).and_then(|v| v.as_f64()) {
                                    h = val as i64;
                                }
                            }
                        }
                    }
                }

                let bid = openrtb::Bid {
                    id: decision.ad_id.to_string(),
                    impid: imp.id.clone(),
                    price: cpm,
                    adm: Some(creative),
                    crid: Some(decision.creative_id.to_string()),
                    w: Some(w as i32),
                    h: Some(h as i32),
                    ..Default::default()
                };

                result.bids.push(TypedBid::new(bid, BidType::Banner));
            }
        }

        Ok(result)
    }
}
