use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct InmobiAdapter {
    pub endpoint: String,
}

impl InmobiAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

/// Get media type from mtype value (from bid ext or response mtype field).
/// InMobi: 1=Banner, 2=Video, 4=Native
fn get_media_type_from_mtype(mtype: u64, bid_id: &str) -> Result<BidType, BidderError> {
    match mtype {
        1 => Ok(BidType::Banner),
        2 => Ok(BidType::Video),
        4 => Ok(BidType::Native),
        other => Err(BidderError::BadServerResponse(format!(
            "Unsupported mtype {} for bid {}", other, bid_id
        ))),
    }
}

impl Bidder for InmobiAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::BadInput("No impression in the request".to_string())]);
        }

        let mut req = request.clone();

        // Preprocess first imp
        {
            let imp = &mut req.imp[0];

            // Validate bidder ext has non-empty plc
            let plc = imp.ext.as_ref()
                .and_then(|e| e.get("bidder"))
                .and_then(|b| b.get("plc"))
                .and_then(|v| v.as_str())
                .unwrap_or("");

            if plc.is_empty() {
                return (vec![], vec![BidderError::BadInput(
                    "'plc' is a required attribute for InMobi's bidder ext".to_string()
                )]);
            }

            // If banner has no w/h but has formats, use first format
            if let Some(banner) = imp.banner.as_mut() {
                let needs_size = banner.w.map(|w| w == 0).unwrap_or(true)
                    || banner.h.map(|h| h == 0).unwrap_or(true);
                if needs_size {
                    let first_format = banner.format.as_ref()
                        .and_then(|f| f.first())
                        .cloned();
                    if let Some(first) = first_format {
                        banner.w = first.w;
                        banner.h = first.h;
                    }
                }
            }
        }

        let body = match serde_json::to_vec(&req) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        let imp_ids = get_imp_ids(&req.imp);
        (
            vec![RequestData {
                method: "POST".to_string(),
                uri: self.endpoint.clone(),
                body,
                headers,
                imp_ids,
            }],
            vec![],
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
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected http status code: {}", response.status_code
            ))]);
        }

        let bid_response: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(1);

        for sb in bid_response.seatbid {
            for bid in sb.bid {
                // mtype is stored in bid.ext.mtype (no direct mtype field on Bid struct)
                let mtype = bid.ext.as_ref()
                    .and_then(|e| e.get("mtype"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);

                let bid_type = get_media_type_from_mtype(mtype, &bid.id)
                    .map_err(|e| vec![e])?;

                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }

        Ok(result)
    }
}
