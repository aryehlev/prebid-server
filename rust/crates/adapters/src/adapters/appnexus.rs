use std::collections::HashMap;

use pbs_adapters::{
    Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use openrtb_ext::{BidType, ExtBidPrebidVideo};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const MAX_IMPS_PER_REQ: usize = 10;

pub struct AppnexusAdapter {
    pub endpoint: String,
}

impl AppnexusAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

/// Appnexus bidder imp ext params
#[derive(Debug, Default, Deserialize)]
struct ExtImpAppnexus {
    #[serde(rename = "placementId", default)]
    placement_id: i64,
    #[serde(rename = "placement_id", default)]
    placement_id_legacy: i64,
    #[serde(rename = "invCode", default)]
    inv_code: String,
    #[serde(rename = "inv_code", default)]
    inv_code_legacy: String,
    #[serde(default)]
    member: String,
    #[serde(default)]
    reserve: f64,
}

#[derive(Debug, Deserialize)]
struct ImpExt {
    bidder: ExtImpAppnexus,
    #[serde(default)]
    gpid: String,
}

/// Appnexus bid ext for determining bid type
#[derive(Debug, Default, Deserialize)]
struct AppnexusBidExt {
    appnexus: AppnexusBidExtInner,
}

#[derive(Debug, Default, Deserialize)]
struct AppnexusBidExtInner {
    #[serde(rename = "bid_ad_type", default)]
    bid_ad_type: i32,
    #[serde(rename = "deal_priority", default)]
    deal_priority: i32,
    #[serde(rename = "brand_category_id", default)]
    brand_category: i32,
    #[serde(rename = "creativeInfo", default)]
    creative_info: AppnexusCreativeInfo,
}

#[derive(Debug, Default, Deserialize)]
struct AppnexusCreativeInfo {
    #[serde(default)]
    video: AppnexusVideoCreativeInfo,
}

#[derive(Debug, Default, Deserialize)]
struct AppnexusVideoCreativeInfo {
    #[serde(default)]
    duration: i32,
}

fn get_bid_type_from_appnexus(bid_type: i32) -> Result<BidType, BidderError> {
    match bid_type {
        0 => Ok(BidType::Banner),
        1 => Ok(BidType::Video),
        3 => Ok(BidType::Native),
        _ => Err(BidderError::BadServerResponse(format!(
            "Unrecognized bid_ad_type in response from appnexus: {}",
            bid_type
        ))),
    }
}

fn build_endpoint_with_member(base: &str, member_id: &str) -> String {
    if member_id.is_empty() {
        return base.to_string();
    }
    if base.contains('?') {
        format!("{}&member_id={}", base, member_id)
    } else {
        format!("{}?member_id={}", base, member_id)
    }
}

impl Bidder for AppnexusAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut valid_imps = Vec::new();
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

            let mut bidder = imp_ext.bidder;
            // Handle legacy param names
            if bidder.placement_id == 0 && bidder.placement_id_legacy != 0 {
                bidder.placement_id = bidder.placement_id_legacy;
            }
            if bidder.inv_code.is_empty() && !bidder.inv_code_legacy.is_empty() {
                bidder.inv_code = bidder.inv_code_legacy.clone();
            }

            // Validate: need placement OR (member + invcode)
            if bidder.placement_id == 0 && (bidder.inv_code.is_empty() || bidder.member.is_empty()) {
                errs.push(BidderError::BadInput(
                    "No placement or member+invcode provided".to_string(),
                ));
                continue;
            }

            // Validate member ID consistency
            if !bidder.member.is_empty() {
                if unique_member_id.is_empty() {
                    unique_member_id = bidder.member.clone();
                } else if unique_member_id != bidder.member {
                    errs.push(BidderError::BadInput(format!(
                        "all request.imp[i].ext.prebid.bidder.appnexus.member params must match. \
                         Request contained member IDs {} and {}",
                        unique_member_id, bidder.member
                    )));
                    return (vec![], errs);
                }
            }

            let mut imp_copy = imp.clone();

            // Set tag ID from inv_code if present
            if !bidder.inv_code.is_empty() {
                imp_copy.tagid = Some(bidder.inv_code.clone());
            }

            // Set bid floor from reserve if imp has none
            if imp_copy.bidfloor.unwrap_or(0.0) <= 0.0 && bidder.reserve > 0.0 {
                imp_copy.bidfloor = Some(bidder.reserve);
            }

            // Build appnexus imp ext
            let new_ext = serde_json::json!({
                "appnexus": {
                    "placement_id": bidder.placement_id,
                },
                "gpid": imp_ext.gpid
            });
            imp_copy.ext = Some(new_ext);

            valid_imps.push(imp_copy);
        }

        if valid_imps.is_empty() {
            return (vec![], errs);
        }

        let uri = build_endpoint_with_member(&self.endpoint, &unique_member_id);

        // Determine AMP/VIDEO entry point
        let is_amp: i32 = if req_info.pbs_entry_point == "amp" { 1 } else { 0 };
        let is_video: i32 = if req_info.pbs_entry_point == "video" { 1 } else { 0 };

        // Build request ext with appnexus section
        let req_ext = serde_json::json!({
            "appnexus": {
                "is_amp": is_amp,
                "hb_source": 5 + is_video,
            }
        });

        let mut headers = HashMap::new();
        headers.insert(
            "Content-Type".to_string(),
            "application/json;charset=utf-8".to_string(),
        );
        headers.insert("Accept".to_string(), "application/json".to_string());

        // Split into chunks of MAX_IMPS_PER_REQ
        let mut requests = Vec::new();
        let chunks = valid_imps.chunks(MAX_IMPS_PER_REQ);

        for chunk in chunks {
            let mut req_copy = request.clone();
            req_copy.imp = chunk.to_vec();
            req_copy.ext = Some(req_ext.clone());

            let body = match serde_json::to_vec(&req_copy) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(BidderError::BadInput(e.to_string()));
                    continue;
                }
            };

            let imp_ids: Vec<String> = req_copy.imp.iter().map(|i| i.id.clone()).collect();

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

        let bid_response: openrtb::BidResponse =
            serde_json::from_slice(&response.body)
                .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(5);
        if let Some(cur) = &bid_response.cur {
            if !cur.is_empty() {
                result.currency = cur.clone();
            }
        }

        let mut errs = Vec::new();

        for seat_bid in bid_response.seatbid {
            for bid in seat_bid.bid {
                let bid_ext: AppnexusBidExt = match bid
                    .ext
                    .as_ref()
                    .and_then(|e| serde_json::from_value(e.clone()).ok())
                {
                    Some(e) => e,
                    None => {
                        errs.push(BidderError::BadServerResponse(
                            "Failed to parse appnexus bid ext".to_string(),
                        ));
                        continue;
                    }
                };

                let bid_type = match get_bid_type_from_appnexus(bid_ext.appnexus.bid_ad_type) {
                    Ok(t) => t,
                    Err(e) => {
                        errs.push(e);
                        continue;
                    }
                };

                let bid_video = Some(ExtBidPrebidVideo {
                    duration: bid_ext.appnexus.creative_info.video.duration,
                    primary_category: String::new(),
                });

                let mut typed_bid = TypedBid::new(bid, bid_type);
                typed_bid.bid_video = bid_video;
                typed_bid.deal_priority = bid_ext.appnexus.deal_priority;
                result.bids.push(typed_bid);
            }
        }

        Ok(result)
    }
}
