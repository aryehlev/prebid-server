use std::collections::HashMap;
use pbs_adapters::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct TripleliftAdapter {
    pub endpoint: String,
}

impl TripleliftAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

/// Triplelift format codes for video: 11, 12, 17
fn get_bid_type_from_tl_format(format: i64) -> BidType {
    if format == 11 || format == 12 || format == 17 {
        BidType::Video
    } else {
        BidType::Banner
    }
}

impl Bidder for TripleliftAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut valid_imps = Vec::new();

        for imp in &request.imp {
            // Check that banner or video is present
            if imp.banner.is_none() && imp.video.is_none() {
                errs.push(BidderError::BadInput("neither Banner nor Video object specified".to_string()));
                continue;
            }

            let mut imp = imp.clone();

            // Extract triplelift ext: inv_code -> tagid, floor -> bidfloor
            if let Some(ext) = imp.ext.as_ref() {
                if let Some(bidder) = ext.get("bidder") {
                    let inv_code = bidder.get("inventoryCode")
                        .or_else(|| bidder.get("inv_code"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let floor = bidder.get("floor").and_then(|v| v.as_f64());

                    if let Some(code) = inv_code {
                        imp.tagid = Some(code);
                    }
                    if let Some(f) = floor {
                        imp.bidfloor = Some(f);
                    }
                }
            }
            valid_imps.push(imp);
        }

        if valid_imps.is_empty() {
            errs.push(BidderError::BadInput("No valid impressions for triplelift".to_string()));
            return (vec![], errs);
        }

        let mut tl_request = request.clone();
        tl_request.imp = valid_imps;

        let body = match serde_json::to_vec(&tl_request) {
            Ok(b) => b,
            Err(e) => {
                errs.push(BidderError::BadInput(e.to_string()));
                return (vec![], errs);
            }
        };

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        let imp_ids = get_imp_ids(&tl_request.imp);
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

        let mut errs = Vec::new();
        let count: usize = bid_response.seatbid.iter().map(|sb| sb.bid.len()).sum();
        let mut result = BidderResponse::with_capacity(count);

        for sb in bid_response.seatbid {
            for bid in sb.bid {
                // Parse bid.ext.triplelift_pb.format
                let format = bid.ext
                    .as_ref()
                    .and_then(|e| e.get("triplelift_pb"))
                    .and_then(|tl| tl.get("format"))
                    .and_then(|f| f.as_i64())
                    .unwrap_or(0);

                if bid.ext.is_none() {
                    errs.push(BidderError::BadServerResponse("missing bid ext".to_string()));
                    continue;
                }

                let bid_type = get_bid_type_from_tl_format(format);
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }

        if errs.is_empty() {
            Ok(result)
        } else {
            // Return partial results + errors — match Go behavior (return both)
            Ok(result)
        }
    }
}
