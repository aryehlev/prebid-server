use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_bid_type_from_imp, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;

pub struct UcfunnelAdapter { pub endpoint: String }
impl UcfunnelAdapter {
    pub fn new(endpoint: String) -> Self { Self { endpoint } }
}

#[derive(Debug, Default, Deserialize)]
struct ExtImpUcfunnel {
    #[serde(rename = "partnerid", default)]
    partner_id: String,
    #[serde(rename = "adunitid", default)]
    ad_unit_id: String,
}

#[derive(Debug, Default, Deserialize)]
struct ImpExt {
    bidder: ExtImpUcfunnel,
}

fn get_partner_id(request: &openrtb::BidRequest) -> Result<String, BidderError> {
    let imp = request.imp.first()
        .ok_or_else(|| BidderError::BadInput("No impression in the bid request".to_string()))?;
    let ext: ImpExt = imp.ext.as_ref()
        .and_then(|e| serde_json::from_value(e.clone()).ok())
        .ok_or_else(|| BidderError::BadInput("Failed to parse ucfunnel imp ext".to_string()))?;
    if ext.bidder.partner_id.is_empty() || ext.bidder.ad_unit_id.is_empty() {
        return Err(BidderError::BadInput("No PartnerId or AdUnitId in the bid request".to_string()));
    }
    Ok(ext.bidder.partner_id)
}

fn get_bid_type(request: &openrtb::BidRequest, imp_id: &str) -> BidType {
    for imp in &request.imp {
        if imp.id == imp_id {
            return get_bid_type_from_imp(imp);
        }
    }
    BidType::Native
}

impl Bidder for UcfunnelAdapter {
    fn make_requests(&self, request: &openrtb::BidRequest, _: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::BadInput("No impression in the bid request".to_string())]);
        }

        let partner_id = match get_partner_id(request) {
            Ok(id) => id,
            Err(e) => return (vec![], vec![e]),
        };

        let body = match serde_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json".to_string());

        // URL encode partner_id manually (only alphanumerics and safe chars)
        let encoded = encode_path_segment(&partner_id);
        let uri = format!("{}/{}/request", self.endpoint, encoded);

        (
            vec![RequestData {
                method: "POST".to_string(),
                uri,
                body,
                headers,
                imp_ids: get_imp_ids(&request.imp),
            }],
            vec![],
        )
    }

    fn make_bids(&self, internal: &openrtb::BidRequest, _: &RequestData, response: &ResponseData) -> Result<BidderResponse, Vec<BidderError>> {
        if response.status_code == 204 { return Ok(BidderResponse::new()); }
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
        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        if bid_resp.seatbid.is_empty() {
            return Ok(BidderResponse::new());
        }

        let cap = bid_resp.seatbid.first().map(|sb| sb.bid.len()).unwrap_or(0);
        let mut result = BidderResponse::with_capacity(cap);
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = get_bid_type(internal, &bid.impid);
                // Only include banner and video bids
                if bid_type == BidType::Banner || bid_type == BidType::Video {
                    result.bids.push(TypedBid::new(bid, bid_type));
                }
            }
        }
        Ok(result)
    }
}

/// Simple percent-encoding for path segments.
fn encode_path_segment(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9'
            | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            other => {
                use std::fmt::Write;
                let _ = write!(out, "%{:02X}", other);
            }
        }
    }
    out
}
