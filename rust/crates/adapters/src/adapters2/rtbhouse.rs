use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

const BIDDER_CURRENCY: &str = "USD";

pub struct RtbhouseAdapter {
    pub endpoint: String,
}

impl RtbhouseAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

/// Get bid type from bid.mtype. RTBHouse uses mtype: 1=Banner, 4=Native
fn get_bid_type_for_bid(bid: &openrtb::Bid) -> Result<BidType, BidderError> {
    match bid.mtype.unwrap_or(0) {
        1 => Ok(BidType::Banner),
        4 => Ok(BidType::Native),
        _ => Err(BidderError::BadServerResponse(format!(
            "unrecognized bid type in response from rtbhouse for bid {}", bid.impid
        ))),
    }
}

/// For native bids, if adm wraps the native object in a "native" key, unwrap it.
fn get_native_adm(adm: &str) -> Result<String, BidderError> {
    let parsed: serde_json::Value = serde_json::from_str(adm)
        .map_err(|_| BidderError::BadServerResponse("unable to unmarshal native adm".to_string()))?;
    if let Some(native_val) = parsed.get("native") {
        serde_json::to_string(native_val)
            .map_err(|_| BidderError::BadServerResponse("unable to get native adm".to_string()))
    } else {
        Ok(adm.to_string())
    }
}

/// Set publisher ext (with prebid.publisherId) in site or app, preserving existing publisher fields.
/// Creates a site object if neither site nor app exists.
fn set_publisher_id(req: &mut openrtb::BidRequest, pub_ext: serde_json::Value) {
    let make_publisher = |existing: Option<&openrtb::Publisher>| -> openrtb::Publisher {
        let mut p = existing.cloned().unwrap_or_default();
        // Merge ext: preserve existing keys, add/overwrite "prebid"
        if let Some(prebid) = pub_ext.get("prebid") {
            let mut ext_obj = if let Some(serde_json::Value::Object(m)) = p.ext.take() {
                m
            } else {
                serde_json::Map::new()
            };
            ext_obj.insert("prebid".to_string(), prebid.clone());
            p.ext = Some(serde_json::Value::Object(ext_obj));
        }
        p
    };

    if let Some(site) = req.site.as_mut() {
        let publisher = make_publisher(site.publisher.as_ref());
        site.publisher = Some(publisher);
    } else if let Some(app) = req.app.as_mut() {
        let publisher = make_publisher(app.publisher.as_ref());
        app.publisher = Some(publisher);
    } else {
        let publisher = make_publisher(None);
        req.site = Some(openrtb::Site {
            publisher: Some(publisher),
            ..Default::default()
        });
    }
}

impl Bidder for RtbhouseAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut req_copy = request.clone();
        req_copy.imp = Vec::new();
        let mut publisher_id = String::new();

        for imp in &request.imp {
            // Extract publisherId and optional bidFloor from bidder ext
            let bidder_ext = imp.ext.as_ref().and_then(|e| e.get("bidder")).cloned();
            if publisher_id.is_empty() {
                if let Some(pid) = bidder_ext.as_ref()
                    .and_then(|b| b.get("publisherId"))
                    .and_then(|v| v.as_str())
                {
                    if !pid.is_empty() {
                        publisher_id = pid.to_string();
                    }
                }
            }

            let mut imp = imp.clone();

            // Apply bidFloor from bidder ext when imp has no floor set
            if imp.bidfloor.is_none() || imp.bidfloor == Some(0.0) {
                if let Some(ext_floor) = bidder_ext.as_ref()
                    .and_then(|b| b.get("bidfloor"))
                    .and_then(|v| v.as_f64())
                {
                    if ext_floor > 0.0 {
                        imp.bidfloor = Some(ext_floor);
                        imp.bidfloorcur = Some(BIDDER_CURRENCY.to_string());
                    }
                }
            }

            // Clear PAAPI/auction environment signals from imp.ext
            if let Some(ext) = imp.ext.as_mut() {
                if let Some(obj) = ext.as_object_mut() {
                    obj.remove("ae");
                    obj.remove("igs");
                    obj.remove("paapi");
                }
            }

            // Remove PMP
            imp.pmp = None;

            req_copy.cur = Some(vec![BIDDER_CURRENCY.to_string()]);
            req_copy.imp.push(imp);
        }

        // Set publisher ID in site/app publisher.ext.prebid.publisherId
        if !publisher_id.is_empty() {
            let pub_ext = serde_json::json!({
                "prebid": { "publisherId": publisher_id }
            });
            set_publisher_id(&mut req_copy, pub_ext);
        }

        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => {
                errs.push(BidderError::BadInput(e.to_string()));
                return (vec![], errs);
            }
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());

        let imp_ids = get_imp_ids(&req_copy.imp);
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

        let bid_response: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let bids_capacity = bid_response.seatbid.first()
            .map(|sb| sb.bid.len())
            .unwrap_or(0);
        let mut result = BidderResponse::with_capacity(bids_capacity);
        result.currency = BIDDER_CURRENCY.to_string();

        let mut errs = Vec::new();

        for sb in bid_response.seatbid {
            for mut bid in sb.bid {
                // Resolve macros
                let price_str = format!("{}", bid.price);
                if let Some(nurl) = bid.nurl.as_mut() {
                    *nurl = nurl.replace("${AUCTION_PRICE}", &price_str);
                }
                if let Some(adm) = bid.adm.as_mut() {
                    *adm = adm.replace("${AUCTION_PRICE}", &price_str);
                }

                let bid_type = match get_bid_type_for_bid(&bid) {
                    Ok(t) => t,
                    Err(e) => {
                        errs.push(e);
                        continue;
                    }
                };

                if bid_type == BidType::Native {
                    if let Some(adm) = bid.adm.as_ref() {
                        match get_native_adm(adm) {
                            Ok(new_adm) => bid.adm = Some(new_adm),
                            Err(e) => {
                                errs.push(e);
                                continue;
                            }
                        }
                    }
                }

                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }

        Ok(result)
    }
}
