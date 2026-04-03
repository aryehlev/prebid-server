use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use openrtb_ext::BidType;
use serde::{Deserialize, Serialize};

pub struct InvibesAdapter { pub endpoint: String }
impl InvibesAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

const ADAPTER_VERSION: &str = "prebid_1.0.0";
const INVIBES_BID_VERSION: &str = "4";

#[derive(Debug, Deserialize, Clone)]
struct ExtImpInvibes {
    #[serde(rename = "placementId", default)]
    placement_id: String,
    #[serde(rename = "domainId", default)]
    domain_id: i32,
    #[serde(default)]
    debug: ExtImpInvibesDebug,
}

#[derive(Debug, Deserialize, Clone, Default)]
struct ExtImpInvibesDebug {
    #[serde(rename = "testBvid", default)]
    test_bvid: String,
    #[serde(rename = "testLog", default)]
    test_log: bool,
}

#[derive(Debug, Serialize, Clone)]
struct InvibesAdRequest {
    #[serde(rename = "BidParamsJson")]
    bid_params_json: String,
    #[serde(rename = "Location")]
    location: String,
    #[serde(rename = "Lid")]
    lid: String,
    #[serde(rename = "IsTestBid")]
    is_test_bid: bool,
    #[serde(rename = "Kw")]
    kw: String,
    #[serde(rename = "IsAmp")]
    is_amp: bool,
    #[serde(rename = "Width")]
    width: String,
    #[serde(rename = "Height")]
    height: String,
    #[serde(rename = "GdprConsent")]
    gdpr_consent: String,
    #[serde(rename = "Gdpr")]
    gdpr: bool,
    #[serde(rename = "Bvid")]
    bvid: String,
    #[serde(rename = "InvibBVLog")]
    invib_bv_log: bool,
    #[serde(rename = "VideoAdDebug")]
    video_ad_debug: bool,
}

#[derive(Debug, Serialize, Clone)]
struct InvibesBidParams {
    #[serde(rename = "PlacementIds")]
    placement_ids: Vec<String>,
    #[serde(rename = "BidVersion")]
    bid_version: String,
    #[serde(rename = "Properties")]
    properties: HashMap<String, InvibesPlacementProperty>,
}

#[derive(Debug, Serialize, Clone)]
struct InvibesPlacementProperty {
    #[serde(rename = "Formats")]
    formats: Vec<InvibesFormat>,
    #[serde(rename = "ImpId")]
    imp_id: String,
}

#[derive(Debug, Serialize, Clone)]
struct InvibesFormat {
    #[serde(rename = "W", skip_serializing_if = "Option::is_none")]
    w: Option<i32>,
    #[serde(rename = "H", skip_serializing_if = "Option::is_none")]
    h: Option<i32>,
}

#[derive(Debug, Deserialize, Clone)]
struct BidServerBidderResponse {
    #[serde(default)]
    currency: String,
    #[serde(rename = "typedBids", default)]
    typed_bids: Vec<BidServerTypedBid>,
    #[serde(default)]
    error: String,
}

#[derive(Debug, Deserialize, Clone)]
struct BidServerTypedBid {
    bid: InvibesBid,
    #[serde(rename = "dealPriority", default)]
    deal_priority: i32,
}

#[derive(Debug, Deserialize, Clone, Default)]
struct InvibesBid {
    #[serde(default)]
    id: String,
    #[serde(default)]
    impid: String,
    #[serde(default)]
    price: f64,
    #[serde(default)]
    adid: String,
    #[serde(default)]
    adm: String,
    #[serde(default)]
    adomain: Vec<String>,
    #[serde(default)]
    iurl: String,
    #[serde(default)]
    cid: String,
    #[serde(default)]
    crid: String,
    #[serde(default)]
    w: Option<i32>,
    #[serde(default)]
    h: Option<i32>,
}

fn make_subdomain(domain_id: i32) -> String {
    if domain_id == 0 || domain_id == 1 || domain_id == 1001 {
        "bid".to_string()
    } else if domain_id < 1002 {
        format!("bid{}", domain_id)
    } else {
        format!("bid{}", domain_id - 1000)
    }
}

impl Bidder for InvibesAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut placement_ids: Vec<String> = Vec::new();
        let mut properties: HashMap<String, InvibesPlacementProperty> = HashMap::new();
        let mut domain_id: i32 = 0;
        let mut test_bvid = String::new();
        let mut test_log = false;

        for imp in &request.imp {
            let bidder_val = match imp.ext.as_ref().and_then(|e| e.get("bidder")) {
                Some(v) => v.clone(),
                None => {
                    errs.push(BidderError::BadInput("Error parsing bidderExt object".to_string()));
                    continue;
                }
            };
            let invibes_ext: ExtImpInvibes = match serde_json::from_value(bidder_val) {
                Ok(e) => e,
                Err(_) => {
                    errs.push(BidderError::BadInput("Error parsing invibesExt parameters".to_string()));
                    continue;
                }
            };

            if imp.banner.is_none() {
                errs.push(BidderError::BadInput("Banner not specified".to_string()));
                continue;
            }

            // Read ad formats from banner
            let mut formats: Vec<InvibesFormat> = Vec::new();
            if let Some(banner) = &imp.banner {
                if let Some(fmt_list) = &banner.format {
                    for f in fmt_list {
                        formats.push(InvibesFormat { w: f.w, h: f.h });
                    }
                } else if banner.w.is_some() && banner.h.is_some() {
                    formats.push(InvibesFormat { w: banner.w, h: banner.h });
                }
            }

            domain_id = invibes_ext.domain_id;
            let placement_id = invibes_ext.placement_id.trim().to_string();
            placement_ids.push(placement_id.clone());
            properties.insert(placement_id, InvibesPlacementProperty {
                imp_id: imp.id.clone(),
                formats,
            });

            if !invibes_ext.debug.test_bvid.is_empty() {
                test_bvid = invibes_ext.debug.test_bvid.clone();
            }
            test_log = invibes_ext.debug.test_log;
        }

        if placement_ids.is_empty() {
            return (vec![], errs);
        }

        // Build URL: replace {{.ZoneID}} in template with subdomain
        let subdomain = make_subdomain(domain_id);
        let url = self.endpoint.replace("{{.ZoneID}}", &subdomain);

        // GDPR
        let (gdpr, gdpr_consent) = {
            let consent = request.user.as_ref()
                .and_then(|u| u.ext.as_ref())
                .and_then(|e| e.get("consent"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let gdpr_applies = request.regs.as_ref()
                .and_then(|r| r.ext.as_ref())
                .and_then(|e| e.get("gdpr"))
                .and_then(|v| v.as_i64())
                .map(|v| v == 1)
                .unwrap_or(true);
            (gdpr_applies, consent)
        };

        let lid = request.user.as_ref()
            .and_then(|u| u.buyeruid.as_deref())
            .unwrap_or("")
            .to_string();

        let site_page = request.site.as_ref()
            .and_then(|s| s.page.as_deref())
            .unwrap_or("")
            .to_string();
        let site_keywords = request.site.as_ref()
            .and_then(|s| s.keywords.as_deref())
            .unwrap_or("")
            .to_string();

        let (width, height) = request.device.as_ref()
            .map(|d| (
                d.w.map(|v| v.to_string()).unwrap_or_default(),
                d.h.map(|v| v.to_string()).unwrap_or_default(),
            ))
            .unwrap_or_default();

        let bid_params = InvibesBidParams {
            placement_ids: placement_ids.clone(),
            bid_version: INVIBES_BID_VERSION.to_string(),
            properties: properties.clone(),
        };

        let bid_params_json = match serde_json::to_string(&bid_params) {
            Ok(s) => s,
            Err(e) => {
                errs.push(BidderError::BadInput(e.to_string()));
                return (vec![], errs);
            }
        };

        let inv_request = InvibesAdRequest {
            is_test_bid: !test_bvid.is_empty(),
            bid_params_json,
            location: site_page.clone(),
            lid,
            kw: site_keywords,
            is_amp: false,
            width,
            height,
            gdpr_consent,
            gdpr,
            bvid: test_bvid,
            invib_bv_log: test_log,
            video_ad_debug: test_log,
        };

        let body = match serde_json::to_vec(&inv_request) {
            Ok(b) => b,
            Err(e) => {
                errs.push(BidderError::BadInput(e.to_string()));
                return (vec![], errs);
            }
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());
        headers.insert("Aver".to_string(), ADAPTER_VERSION.to_string());

        if let Some(device) = &request.device {
            if let Some(ua) = &device.ua {
                headers.insert("User-Agent".to_string(), ua.clone());
            }
            if let Some(ip) = &device.ip {
                headers.insert("X-Forwarded-For".to_string(), ip.clone());
            } else if let Some(ipv6) = &device.ipv6 {
                headers.insert("X-Forwarded-For".to_string(), ipv6.clone());
            }
        }
        if !site_page.is_empty() {
            headers.insert("Referer".to_string(), site_page);
        }

        let imp_ids: Vec<String> = properties.values().map(|p| p.imp_id.clone()).collect();

        (vec![RequestData { method: "POST".to_string(), uri: url, body, headers, imp_ids }], errs)
    }

    fn make_bids(&self, _: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected status code: {}.", response.status_code
            ))]);
        }

        let bid_response: BidServerBidderResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        if !bid_response.error.is_empty() {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Server error: {}.", bid_response.error
            ))]);
        }

        let mut result = BidderResponse::with_capacity(bid_response.typed_bids.len());
        result.currency = bid_response.currency;

        for typed_bid in bid_response.typed_bids {
            let b = typed_bid.bid;
            let bid = openrtb::Bid {
                id: b.id,
                impid: b.impid,
                price: b.price,
                adm: if b.adm.is_empty() { None } else { Some(b.adm) },
                adomain: if b.adomain.is_empty() { None } else { Some(b.adomain) },
                iurl: if b.iurl.is_empty() { None } else { Some(b.iurl) },
                cid: if b.cid.is_empty() { None } else { Some(b.cid) },
                crid: if b.crid.is_empty() { None } else { Some(b.crid) },
                w: b.w,
                h: b.h,
                ..Default::default()
            };
            let mut typed = TypedBid::new(bid, BidType::Banner);
            typed.deal_priority = typed_bid.deal_priority;
            result.bids.push(typed);
        }

        Ok(result)
    }
}
