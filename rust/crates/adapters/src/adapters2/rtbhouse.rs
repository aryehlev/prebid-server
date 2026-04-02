use std::collections::HashMap;
use pbs_adapters::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
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

fn get_bid_type_from_mtype(mtype: u32) -> Result<BidType, BidderError> {
    match mtype {
        1 => Ok(BidType::Banner),
        4 => Ok(BidType::Native),
        _ => Err(BidderError::BadServerResponse(format!(
            "unrecognized bid type in response from rtbhouse"
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
            // Extract bidder ext
            let rtbhouse_ext = imp.ext.as_ref()
                .and_then(|e| e.get("bidder"));

            if let Some(ext) = rtbhouse_ext {
                if publisher_id.is_empty() {
                    if let Some(pid) = ext.get("publisherId").and_then(|v| v.as_str()) {
                        if !pid.is_empty() {
                            publisher_id = pid.to_string();
                        }
                    }
                }
            }

            let mut imp = imp.clone();

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

            // Set currency
            req_copy.cur = vec![BIDDER_CURRENCY.to_string()];
            req_copy.imp.push(imp);
        }

        // Set publisher ID in site/app publisher ext
        if !publisher_id.is_empty() {
            let pub_ext = serde_json::json!({
                "prebid": { "publisherId": publisher_id }
            });
            let publisher = openrtb::Publisher {
                ext: Some(pub_ext),
                ..Default::default()
            };
            if let Some(site) = req_copy.site.as_mut() {
                site.publisher = Some(publisher);
            } else if let Some(app) = req_copy.app.as_mut() {
                app.publisher = Some(publisher);
            } else {
                req_copy.site = Some(openrtb::Site {
                    publisher: Some(publisher),
                    ..Default::default()
                });
            }
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
        if let Err(e) = pbs_adapters::check_response_status(response.status_code) {
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

                let mtype = bid.ext.as_ref()
                    .and_then(|e| e.get("mtype"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32;

                // Also check top-level mtype if available via bid struct
                let bid_mtype = mtype;

                let bid_type = match get_bid_type_from_mtype(bid_mtype) {
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

        if errs.is_empty() {
            Ok(result)
        } else {
            Ok(result)
        }
    }
}
