use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use openrtb_ext::BidType;
use serde::{Deserialize, Serialize};

const MAX_IMPS_PER_REQ: usize = 10;

pub struct AppnexusAdapter {
    pub endpoint: String,
}

impl AppnexusAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

/// Incoming bidder extension from the request
#[derive(Debug, Default, Deserialize)]
struct ExtImpAppnexus {
    #[serde(rename = "member", default)]
    pub member: String,
    #[serde(rename = "placement_id", default)]
    pub placement_id: i64,
    /// Legacy alias
    #[serde(rename = "placementId", default)]
    pub placement_id_legacy: i64,
    #[serde(rename = "inv_code", default)]
    pub inv_code: String,
    #[serde(rename = "invCode", default)]
    pub inv_code_legacy: String,
    #[serde(rename = "reserve", default)]
    pub reserve: f64,
    #[serde(rename = "position", default)]
    pub position: String,
    #[serde(rename = "traffic_source_code", default)]
    pub traffic_source_code: String,
    #[serde(rename = "keywords", default)]
    #[allow(dead_code)]
    pub keywords: serde_json::Value,
    #[serde(rename = "use_pmt_rule", default)]
    pub use_payment_rule: Option<bool>,
    #[serde(rename = "private_sizes", default)]
    #[allow(dead_code)]
    pub private_sizes: Option<serde_json::Value>,
    #[serde(rename = "ext_inv_code", default)]
    pub ext_inv_code: String,
    #[serde(rename = "external_imp_id", default)]
    pub external_imp_id: String,
    #[serde(rename = "generate_ad_pod_id", default)]
    #[allow(dead_code)]
    pub generate_ad_pod_id: bool,
}

#[derive(Debug, Default, Deserialize)]
struct ImpExt {
    #[serde(rename = "bidder", default)]
    bidder: ExtImpAppnexus,
    #[serde(rename = "gpid", default)]
    gpid: String,
}

/// The imp.ext we send to appnexus
#[derive(Serialize)]
struct AppnexusImpExt {
    appnexus: AppnexusImpExtAppnexus,
    #[serde(skip_serializing_if = "String::is_empty")]
    gpid: String,
}

#[derive(Serialize)]
struct AppnexusImpExtAppnexus {
    placement_id: i64,
    #[serde(skip_serializing_if = "String::is_empty")]
    traffic_source_code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    use_pmt_rule: Option<bool>,
    #[serde(skip_serializing_if = "String::is_empty")]
    ext_inv_code: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    external_imp_id: String,
}

/// bid.ext from appnexus response
#[derive(Debug, Default, Deserialize)]
struct BidExt {
    #[serde(rename = "appnexus", default)]
    appnexus: BidExtAppnexus,
}

#[derive(Debug, Default, Deserialize)]
struct BidExtAppnexus {
    /// 0=banner, 1=video, 3=native
    #[serde(rename = "bid_ad_type", default)]
    bid_ad_type: i32,
    #[serde(rename = "brand_category_id", default)]
    #[allow(dead_code)]
    brand_category: i32,
    #[serde(rename = "deal_priority", default)]
    deal_priority: i32,
    #[serde(rename = "creative_info", default)]
    creative_info: CreativeInfo,
}

#[derive(Debug, Default, Deserialize)]
struct CreativeInfo {
    #[serde(rename = "video", default)]
    video: CreativeInfoVideo,
}

#[derive(Debug, Default, Deserialize)]
struct CreativeInfoVideo {
    #[serde(rename = "duration", default)]
    duration: i32,
}

fn get_bid_type_from_appnexus(bid_ad_type: i32) -> Result<BidType, BidderError> {
    match bid_ad_type {
        0 => Ok(BidType::Banner),
        1 => Ok(BidType::Video),
        3 => Ok(BidType::Native),
        t => Err(BidderError::BadServerResponse(format!(
            "Unrecognized bid_ad_type in response from appnexus: {}",
            t
        ))),
    }
}

/// Validate that placement_id OR (inv_code + member) is provided
fn validate_appnexus_ext(ext: &ExtImpAppnexus) -> Result<(), BidderError> {
    if ext.placement_id == 0 && (ext.inv_code.is_empty() || ext.member.is_empty()) {
        return Err(BidderError::BadInput(
            "No placement or member+invcode provided".to_string(),
        ));
    }
    Ok(())
}

fn get_imp_ids(imps: &[openrtb::Imp]) -> Vec<String> {
    imps.iter().map(|i| i.id.clone()).collect()
}

impl Bidder for AppnexusAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut valid_imps: Vec<openrtb::Imp> = Vec::new();
        let mut unique_member_id = String::new();

        for imp in &request.imp {
            let imp_ext: ImpExt = match imp
                .ext
                .as_ref()
                .and_then(|e| serde_json::from_value(e.clone()).ok())
            {
                Some(e) => e,
                None => {
                    errs.push(BidderError::BadInput(
                        "Failed to parse appnexus imp ext".to_string(),
                    ));
                    continue;
                }
            };

            // Handle legacy params
            let mut bidder = imp_ext.bidder;
            if bidder.placement_id == 0 && bidder.placement_id_legacy != 0 {
                bidder.placement_id = bidder.placement_id_legacy;
            }
            if bidder.inv_code.is_empty() && !bidder.inv_code_legacy.is_empty() {
                bidder.inv_code = bidder.inv_code_legacy.clone();
            }

            if let Err(e) = validate_appnexus_ext(&bidder) {
                errs.push(e);
                continue;
            }

            let member_id = bidder.member.clone();
            if !member_id.is_empty() {
                if unique_member_id.is_empty() {
                    unique_member_id = member_id;
                } else if unique_member_id != member_id {
                    errs.push(BidderError::BadInput(format!(
                        "all request.imp[i].ext.prebid.bidder.appnexus.member params must match. Request contained member IDs {} and {}",
                        unique_member_id, member_id
                    )));
                    return (vec![], errs);
                }
            }

            // Build outgoing imp
            let mut imp_copy = imp.clone();

            // Set tagid from inv_code
            if !bidder.inv_code.is_empty() {
                imp_copy.tagid = Some(bidder.inv_code.clone());
            }

            // Set bid floor from reserve if not set
            if imp_copy.bidfloor.unwrap_or(0.0) <= 0.0 && bidder.reserve > 0.0 {
                imp_copy.bidfloor = Some(bidder.reserve);
            }

            // Handle banner position
            if let Some(banner) = imp_copy.banner.as_mut() {
                let pos = match bidder.position.as_str() {
                    "above" => Some(1i32), // ATF
                    "below" => Some(3i32), // BTF
                    _ => None,
                };
                if let Some(p) = pos {
                    banner.pos = Some(p);
                }
                // Populate w/h from first format if missing
                if banner.w.is_none() && banner.h.is_none() {
                    if let Some(formats) = banner.format.as_ref() {
                        if let Some(first) = formats.first() {
                            banner.w = first.w;
                            banner.h = first.h;
                        }
                    }
                }
            }

            // Build the appnexus imp ext
            let appnexus_imp_ext = AppnexusImpExt {
                appnexus: AppnexusImpExtAppnexus {
                    placement_id: bidder.placement_id,
                    traffic_source_code: bidder.traffic_source_code.clone(),
                    use_pmt_rule: bidder.use_payment_rule,
                    ext_inv_code: bidder.ext_inv_code.clone(),
                    external_imp_id: bidder.external_imp_id.clone(),
                },
                gpid: imp_ext.gpid,
            };

            match serde_json::to_value(&appnexus_imp_ext) {
                Ok(v) => imp_copy.ext = Some(v),
                Err(e) => {
                    errs.push(BidderError::BadInput(e.to_string()));
                    continue;
                }
            }

            valid_imps.push(imp_copy);
        }

        if valid_imps.is_empty() {
            return (vec![], errs);
        }

        // Build endpoint URI with member_id if present
        let uri = if !unique_member_id.is_empty() {
            if self.endpoint.contains('?') {
                format!("{}&member_id={}", self.endpoint, unique_member_id)
            } else {
                format!("{}?member_id={}", self.endpoint, unique_member_id)
            }
        } else {
            self.endpoint.clone()
        };

        let mut headers = HashMap::new();
        headers.insert(
            "Content-Type".to_string(),
            "application/json;charset=utf-8".to_string(),
        );
        headers.insert("Accept".to_string(), "application/json".to_string());

        let mut requests = Vec::new();

        // Split into batches of MAX_IMPS_PER_REQ
        for chunk in valid_imps.chunks(MAX_IMPS_PER_REQ) {
            let mut req_copy = request.clone();
            req_copy.imp = chunk.to_vec();

            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(BidderError::BadInput(e.to_string()));
                    return (vec![], errs);
                }
            };

            let imp_ids = get_imp_ids(&req_copy.imp);

            requests.push(RequestData {
                method: "POST".to_string(),
                uri: uri.clone(),
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
        if let Err(e) = crate::check_response_status(response.status_code) {
            return Err(vec![e]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(5);
        let mut errs = Vec::new();

        if bid_resp.cur.as_deref().unwrap_or("") != "" {
            result.currency = bid_resp.cur.clone().unwrap_or_else(|| "USD".to_string());
        }

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_ext: BidExt = bid
                    .ext
                    .as_ref()
                    .and_then(|e| serde_json::from_value(e.clone()).ok())
                    .unwrap_or_default();

                let bid_type = match get_bid_type_from_appnexus(bid_ext.appnexus.bid_ad_type) {
                    Ok(t) => t,
                    Err(e) => {
                        errs.push(e);
                        continue;
                    }
                };

                let deal_priority = bid_ext.appnexus.deal_priority;
                let video_duration = bid_ext.appnexus.creative_info.video.duration;

                let mut typed_bid = TypedBid::new(bid, bid_type);
                typed_bid.deal_priority = deal_priority;
                if video_duration > 0 {
                    typed_bid.bid_video = Some(openrtb_ext::ExtBidPrebidVideo {
                        duration: video_duration,
                        primary_category: String::new(),
                    });
                }

                result.bids.push(typed_bid);
            }
        }

        if !errs.is_empty() {
            // Return partial results with errors (like Go does)
            // Since Result<BidderResponse, Vec<BidderError>> forces a choice,
            // we only return Err if we have NO bids at all
            if result.bids.is_empty() {
                return Err(errs);
            }
        }

        Ok(result)
    }
}
