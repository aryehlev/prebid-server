use std::collections::HashMap;
use pbs_adapters::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;

pub struct KayzenAdapter {
    pub endpoint: String,
}

impl KayzenAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

#[derive(Deserialize, Default)]
struct KayzenImpExt {
    #[serde(rename = "zone", default)]
    zone: String,
    #[serde(rename = "exchange", default)]
    exchange: String,
}

/// Determine bid type from the matching impression.
fn get_media_type_for_imp(imp_id: &str, imps: &[openrtb::Imp]) -> BidType {
    for imp in imps {
        if imp.id == imp_id {
            if imp.banner.is_some() {
                return BidType::Banner;
            }
            if imp.video.is_some() {
                return BidType::Video;
            }
            if imp.native.is_some() {
                return BidType::Native;
            }
        }
    }
    BidType::Banner
}

impl Bidder for KayzenAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (
                vec![],
                vec![BidderError::BadInput("Missing Imp Object".to_string())],
            );
        }

        let mut req_copy = request.clone();

        // Extract ext from first imp, then clear it
        let ext = match req_copy.imp[0].ext.as_ref() {
            Some(e) => e.clone(),
            None => {
                return (
                    vec![],
                    vec![BidderError::BadInput(
                        "Bidder extension not provided or can't be unmarshalled".to_string(),
                    )],
                );
            }
        };

        let bidder_val = match ext.get("bidder") {
            Some(v) => v.clone(),
            None => {
                return (
                    vec![],
                    vec![BidderError::BadInput(
                        "Bidder extension not provided or can't be unmarshalled".to_string(),
                    )],
                );
            }
        };

        let kayzen_ext: KayzenImpExt = match serde_json::from_value(bidder_val) {
            Ok(v) => v,
            Err(_) => {
                return (
                    vec![],
                    vec![BidderError::BadInput(
                        "Error while unmarshaling bidder extension".to_string(),
                    )],
                );
            }
        };

        // Clear ext on first impression
        req_copy.imp[0].ext = None;

        // Build endpoint URL by substituting {{.ZoneID}} and {{.AccountID}} macros
        let uri = self
            .endpoint
            .replace("{{.ZoneID}}", &kayzen_ext.zone)
            .replace("{{.AccountID}}", &kayzen_ext.exchange);

        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let mut headers = HashMap::new();
        headers.insert(
            "Content-Type".to_string(),
            "application/json;charset=utf-8".to_string(),
        );
        headers.insert("Accept".to_string(), "application/json".to_string());

        (
            vec![RequestData {
                method: "POST".to_string(),
                uri,
                body,
                headers,
                imp_ids: get_imp_ids(&req_copy.imp),
            }],
            vec![],
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
        if response.status_code == 400 {
            return Err(vec![BidderError::BadInput(
                "Unexpected status code: 400. Bad request from publisher. Run with request.debug = 1 for more info.".to_string(),
            )]);
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info.",
                response.status_code
            ))]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|_| vec![BidderError::BadServerResponse("Bad Server Response".to_string())])?;

        let mut result = BidderResponse::with_capacity(1);

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = get_media_type_for_imp(&bid.impid, &internal.imp);
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }

        Ok(result)
    }
}
