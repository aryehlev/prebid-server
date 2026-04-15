use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;
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

/// Smartadserver bidder extension from imp.ext.bidder
#[derive(Debug, Default, Deserialize)]
#[allow(dead_code)]
struct ExtImpSmartadserver {
    #[serde(rename = "networkId", default)]
    network_id: i64,
    #[serde(rename = "siteId", default)]
    site_id: i64,
    #[serde(rename = "pageId", default)]
    page_id: i64,
    #[serde(rename = "formatId", default)]
    format_id: i64,
    #[serde(rename = "target", default)]
    target: String,
    #[serde(rename = "bidfloor", default)]
    bid_floor: f64,
    #[serde(rename = "programmaticGuaranteed", default)]
    programmatic_guaranteed: bool,
    #[serde(rename = "buId", default)]
    bu_id: String,
    #[serde(rename = "aDomain", default)]
    a_domain: Vec<String>,
}

fn get_bid_type_from_mtype(mtype: i32) -> BidType {
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

            let smartadserver_ext: ExtImpSmartadserver = match serde_json::from_value(bidder_ext.clone()) {
                Ok(v) => v,
                Err(_) => {
                    errs.push(BidderError::BadInput("Error parsing smartadserverExt parameters".to_string()));
                    continue;
                }
            };

            if !is_programmatic_guaranteed && smartadserver_ext.programmatic_guaranteed {
                is_programmatic_guaranteed = true;
                imp_ext_key = "smartadserver";
            }

            if network_id.is_empty() && smartadserver_ext.network_id != 0 {
                network_id = smartadserver_ext.network_id.to_string();
            }

            // Build the output ext from the parsed extension struct
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

        let bid_response: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(5);
        for sb in bid_response.seatbid {
            for bid in sb.bid {
                // Use bid.mtype directly (OpenRTB 2.6 field)
                let mtype = bid.mtype.unwrap_or(0);
                let bid_type = get_bid_type_from_mtype(mtype);
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_req() -> openrtb::BidRequest {
        openrtb::BidRequest {
            id: "r".to_string(),
            imp: vec![openrtb::Imp {
                id: "i1".to_string(),
                banner: Some(openrtb::Banner {
                    w: Some(300),
                    h: Some(250),
                    ..Default::default()
                }),
                ext: Some(serde_json::json!({
                    "bidder": {"networkId": 73, "siteId": 1, "pageId": 1, "formatId": 1}
                })),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn test_smartadserver_url_and_headers() {
        let adapter = SmartadserverAdapter::new("https://ssb-global.smartadserver.com".to_string());
        let (reqs, _) = adapter.make_requests(&make_req(), &ExtraRequestInfo::default());
        assert_eq!(reqs.len(), 1);
        assert_eq!(
            reqs[0].uri,
            "https://ssb-global.smartadserver.com/api/bid?callerId=5"
        );
        assert_eq!(
            reqs[0].headers.get("Content-Type").unwrap(),
            "application/json;charset=utf-8"
        );
        assert!(!reqs[0].body.is_empty());
    }

    #[test]
    fn test_smartadserver_make_bids() {
        let adapter = SmartadserverAdapter::new("https://ssb-global.smartadserver.com".to_string());
        let body = br#"{"id":"r","seatbid":[{"bid":[{"id":"b","impid":"i1","price":1.9,"crid":"c","mtype":1}]}]}"#;
        let resp = ResponseData::new(200, body.to_vec());
        let result = adapter
            .make_bids(&make_req(), &RequestData::default(), &resp)
            .unwrap();
        assert_eq!(result.bids.len(), 1);
        assert_eq!(result.bids[0].bid_type, BidType::Banner);
    }
}
