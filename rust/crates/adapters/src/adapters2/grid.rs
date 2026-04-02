use std::collections::HashMap;
use pbs_adapters::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde_json::Value;

pub struct GridAdapter {
    pub endpoint: String,
}

impl GridAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

fn get_media_type_for_imp(imp_id: &str, imps: &[openrtb::Imp], content_type: Option<&str>) -> Result<BidType, BidderError> {
    // If the bid itself carries a content_type field, use it directly
    if let Some(ct) = content_type {
        if !ct.is_empty() {
            return match ct {
                "banner" => Ok(BidType::Banner),
                "video" => Ok(BidType::Video),
                "native" => Ok(BidType::Native),
                _ => Ok(BidType::Banner),
            };
        }
    }
    for imp in imps {
        if imp.id == imp_id {
            if imp.banner.is_some() {
                return Ok(BidType::Banner);
            } else if imp.video.is_some() {
                return Ok(BidType::Video);
            } else if imp.native.is_some() {
                return Ok(BidType::Native);
            }
            return Err(BidderError::BadServerResponse(format!(
                "Unknown impression type for ID: \"{}\"", imp_id
            )));
        }
    }
    Err(BidderError::BadServerResponse(format!(
        "Failed to find impression for ID: \"{}\"", imp_id
    )))
}

fn get_bid_meta(ext: &Option<Value>) -> Option<openrtb_ext::ExtBidPrebidMeta> {
    let ext = ext.as_ref()?;
    let demand_source = ext
        .get("bidder")
        .and_then(|b| b.get("grid"))
        .and_then(|g| g.get("demandSource"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if !demand_source.is_empty() {
        Some(openrtb_ext::ExtBidPrebidMeta {
            network_name: Some(demand_source.to_string()),
            ..Default::default()
        })
    } else {
        None
    }
}

impl Bidder for GridAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut valid_imps = Vec::new();

        for imp in &request.imp {
            // Validate: uid must be non-zero in bidder ext
            let uid_ok = imp.ext.as_ref()
                .and_then(|e| e.get("bidder"))
                .and_then(|b| b.get("uid"))
                .and_then(|v| v.as_i64())
                .map(|uid| uid != 0)
                .unwrap_or(false);

            if !uid_ok {
                errs.push(BidderError::BadInput("uid is empty".to_string()));
                continue;
            }

            // setImpExtData: if data.adserver.adslot is set, copy it to gpid
            let mut imp = imp.clone();
            if let Some(ext) = imp.ext.as_mut() {
                if let Some(obj) = ext.as_object_mut() {
                    let adslot = obj.get("data")
                        .and_then(|d| d.get("adserver"))
                        .and_then(|a| a.get("adslot"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    if let Some(slot) = adslot {
                        if !slot.is_empty() {
                            obj.insert("gpid".to_string(), Value::String(slot));
                        }
                    }
                }
            }

            valid_imps.push(imp);
        }

        if valid_imps.is_empty() {
            errs.push(BidderError::BadInput("No valid impressions for grid".to_string()));
            return (vec![], errs);
        }

        let mut grid_request = request.clone();
        grid_request.imp = valid_imps;

        let body = match serde_json::to_vec(&grid_request) {
            Ok(b) => b,
            Err(e) => {
                errs.push(BidderError::BadInput(e.to_string()));
                return (vec![], errs);
            }
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());

        let imp_ids = get_imp_ids(&grid_request.imp);
        (
            vec![RequestData {
                method: "POST".to_string(),
                uri: self.endpoint.clone(),
                body,
                headers,
                imp_ids,
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

        // Grid returns a custom response where bids may have content_type and adm_native
        let bid_response: Value = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(1);

        if let Some(seatbids) = bid_response.get("seatbid").and_then(|v| v.as_array()) {
            for sb in seatbids {
                if let Some(bids) = sb.get("bid").and_then(|v| v.as_array()) {
                    for bid_val in bids {
                        // Extract content_type for bid type resolution
                        let content_type = bid_val.get("content_type").and_then(|v| v.as_str());

                        let imp_id = bid_val.get("impid").and_then(|v| v.as_str()).unwrap_or("");
                        let bid_type = match get_media_type_for_imp(imp_id, &internal.imp, content_type) {
                            Ok(t) => t,
                            Err(e) => return Err(vec![e]),
                        };

                        // Build a standard openrtb::Bid from the value
                        // If adm is empty but adm_native is set, use adm_native as adm
                        let mut bid_obj = bid_val.clone();
                        let adm_is_empty = bid_obj.get("adm")
                            .and_then(|v| v.as_str())
                            .map(|s| s.is_empty())
                            .unwrap_or(true);
                        if adm_is_empty {
                            if let Some(adm_native) = bid_obj.get("adm_native").cloned() {
                                if let Ok(adm_str) = serde_json::to_string(&adm_native) {
                                    if let Some(obj) = bid_obj.as_object_mut() {
                                        obj.insert("adm".to_string(), Value::String(adm_str));
                                    }
                                }
                            }
                        }

                        let bid: openrtb::Bid = match serde_json::from_value(bid_obj) {
                            Ok(b) => b,
                            Err(e) => return Err(vec![BidderError::BadServerResponse(e.to_string())]),
                        };

                        let bid_meta = get_bid_meta(&bid.ext);
                        let mut typed_bid = TypedBid::new(bid, bid_type);
                        typed_bid.bid_meta = bid_meta;
                        result.bids.push(typed_bid);
                    }
                }
            }
        }

        Ok(result)
    }
}
