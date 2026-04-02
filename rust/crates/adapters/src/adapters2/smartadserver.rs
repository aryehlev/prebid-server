use std::collections::HashMap;
use pbs_adapters::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde_json::Value;

pub struct SmartadserverAdapter {
    pub endpoint: String,
}

impl SmartadserverAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }

    fn build_endpoint_url(&self, is_programmatic_guaranteed: bool) -> Result<String, BidderError> {
        if is_programmatic_guaranteed {
            // Use secondary host for programmatic guaranteed
            let secondary = "https://prebid-global.smartadserver.com";
            Ok(format!("{}/ortb", secondary.trim_end_matches('/')))
        } else {
            let base = self.endpoint.trim_end_matches('/');
            Ok(format!("{}/api/bid?callerId=5", base))
        }
    }
}

fn get_bid_type_from_mtype(mtype: u32) -> BidType {
    // openrtb mtype: 1=Banner, 2=Video, 3=Audio, 4=Native
    match mtype {
        2 => BidType::Video,
        3 => BidType::Audio,
        4 => BidType::Native,
        _ => BidType::Banner,
    }
}

impl Bidder for SmartadserverAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::BadInput("No impression in the bid request".to_string())]);
        }

        let mut errs = Vec::new();
        let mut is_programmatic_guaranteed = false;
        let mut imp_ext_key = "bidder";
        let mut network_id = String::new();

        // First pass: extract extensions and detect programmatic guaranteed
        struct PendingImp {
            imp: openrtb::Imp,
            ext_out: Value,
        }
        let mut pending: Vec<PendingImp> = Vec::new();

        for imp in &request.imp {
            let ext = match imp.ext.as_ref() {
                Some(e) => e.clone(),
                None => {
                    errs.push(BidderError::BadInput("Error parsing bidderExt object".to_string()));
                    continue;
                }
            };

            let bidder_ext = match ext.get("bidder") {
                Some(b) => b.clone(),
                None => {
                    errs.push(BidderError::BadInput("Error parsing bidderExt object".to_string()));
                    continue;
                }
            };

            let pg = bidder_ext
                .get("programmaticGuaranteed")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            if !is_programmatic_guaranteed && pg {
                is_programmatic_guaranteed = true;
                imp_ext_key = "smartadserver";
            }

            if let Some(nid) = bidder_ext.get("networkId").and_then(|v| v.as_i64()) {
                if network_id.is_empty() {
                    network_id = nid.to_string();
                }
            }

            // Build an ext_out value from bidder ext fields
            let ext_out = bidder_ext.clone();

            pending.push(PendingImp { imp: imp.clone(), ext_out });
        }

        if pending.is_empty() {
            return (vec![], errs);
        }

        // Rebuild request with modified imps
        let mut smart_request = request.clone();

        // Set publisher id on site
        if !network_id.is_empty() {
            let site = smart_request.site.get_or_insert_with(Default::default);
            let publisher = site.publisher.get_or_insert_with(Default::default);
            publisher.id = Some(network_id.clone());
        }

        let mut valid_imps: Vec<openrtb::Imp> = Vec::new();
        for mut pending_imp in pending {
            // Rebuild imp ext: remove "bidder", add with proper key
            let mut complete_ext: serde_json::Map<String, Value> = match pending_imp.imp.ext.as_ref() {
                Some(Value::Object(m)) => m.clone(),
                _ => serde_json::Map::new(),
            };
            complete_ext.remove("bidder");
            complete_ext.insert(imp_ext_key.to_string(), pending_imp.ext_out);
            pending_imp.imp.ext = Some(Value::Object(complete_ext));
            valid_imps.push(pending_imp.imp);
        }

        smart_request.imp = valid_imps;

        let url = match self.build_endpoint_url(is_programmatic_guaranteed) {
            Ok(u) => u,
            Err(e) => return (vec![], vec![e]),
        };

        let body = match serde_json::to_vec(&smart_request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        let imp_ids = get_imp_ids(&smart_request.imp);
        (
            vec![RequestData {
                method: "POST".to_string(),
                uri: url,
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

        let mut result = BidderResponse::with_capacity(5);
        for sb in bid_response.seatbid {
            for bid in sb.bid {
                let mtype = bid.ext
                    .as_ref()
                    .and_then(|e| e.get("mtype"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32;
                let bid_type = get_bid_type_from_mtype(mtype);
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        Ok(result)
    }
}
