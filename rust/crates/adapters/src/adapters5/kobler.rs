use std::collections::HashMap;
use pbs_adapters::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;

const DEV_ENDPOINT: &str = "https://bid-service.dev.essrtb.com/bid/prebid_server_rtb_call";
const SUPPORTED_CURRENCY: &str = "USD";

pub struct KoblerAdapter {
    pub endpoint: String,
}

impl KoblerAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

#[derive(Deserialize, Default)]
struct KoblerImpExt {
    #[serde(rename = "test", default)]
    test: bool,
}

#[derive(Deserialize, Default)]
struct KoblerBidExtPrebid {
    #[serde(rename = "type", default)]
    bid_type: String,
}

#[derive(Deserialize, Default)]
struct KoblerBidExt {
    #[serde(rename = "prebid")]
    prebid: Option<KoblerBidExtPrebid>,
}

fn get_media_type_for_bid(bid: &openrtb::Bid) -> BidType {
    if let Some(ext) = &bid.ext {
        if let Ok(bid_ext) = serde_json::from_value::<KoblerBidExt>(ext.clone()) {
            if let Some(prebid) = bid_ext.prebid {
                match prebid.bid_type.as_str() {
                    "banner" => return BidType::Banner,
                    "video" => return BidType::Video,
                    "native" => return BidType::Native,
                    _ => {}
                }
            }
        }
    }
    BidType::Banner
}

impl Bidder for KoblerAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut req_copy = request.clone();
        let mut errors: Vec<BidderError> = Vec::new();

        // Sanitize: clear device IP/IPv6 and user
        if let Some(device) = req_copy.device.as_mut() {
            device.ip = None;
            device.ipv6 = None;
        }
        req_copy.user = None;

        // Ensure USD is in the currency list
        let cur = req_copy.cur.get_or_insert_with(Vec::new);
        if !cur.iter().any(|c| c == SUPPORTED_CURRENCY) {
            cur.push(SUPPORTED_CURRENCY.to_string());
        }

        let mut test_mode = false;

        for (i, imp) in req_copy.imp.iter_mut().enumerate() {
            // Convert bid floor currency if needed
            if let (Some(floor), Some(floor_cur)) = (imp.bidfloor, imp.bidfloorcur.as_deref()) {
                if floor > 0.0 && floor_cur.to_uppercase() != SUPPORTED_CURRENCY {
                    match req_info.convert_currency(floor, floor_cur, SUPPORTED_CURRENCY) {
                        Ok(converted) => {
                            imp.bidfloor = Some(converted);
                            imp.bidfloorcur = Some(SUPPORTED_CURRENCY.to_string());
                        }
                        Err(e) => {
                            errors.push(BidderError::BadInput(e.to_string()));
                            return (vec![], errors);
                        }
                    }
                }
            }

            // Check first imp for test mode
            if i == 0 {
                if let Some(ext) = &imp.ext {
                    if let Some(bidder_val) = ext.get("bidder") {
                        if let Ok(kobler_ext) =
                            serde_json::from_value::<KoblerImpExt>(bidder_val.clone())
                        {
                            test_mode = kobler_ext.test;
                        }
                    }
                }
            }
        }

        if !errors.is_empty() {
            return (vec![], errors);
        }

        let body = match serde_json::to_vec(&req_copy) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let endpoint = if test_mode {
            DEV_ENDPOINT.to_string()
        } else {
            self.endpoint.clone()
        };

        let mut headers = HashMap::new();
        headers.insert(
            "Content-Type".to_string(),
            "application/json;charset=utf-8".to_string(),
        );

        (
            vec![RequestData {
                method: "POST".to_string(),
                uri: endpoint,
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
        if response.status_code == 204 || response.body.is_empty() {
            return Ok(BidderResponse::new());
        }
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info.",
                response.status_code
            ))]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(internal.imp.len());
        if let Some(cur) = &bid_resp.cur {
            result.currency = cur.clone();
        }

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = get_media_type_for_bid(&bid);
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }

        Ok(result)
    }
}
