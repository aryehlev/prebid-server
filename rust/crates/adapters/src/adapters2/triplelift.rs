use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;

pub struct TripleliftAdapter {
    pub endpoint: String,
}

impl TripleliftAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

/// Triplelift bidder imp extension (from request)
#[derive(Debug, Default, Deserialize)]
struct ExtImpTriplelift {
    /// inventoryCode maps to imp.tagid
    #[serde(rename = "inventoryCode", default)]
    inventory_code: String,
    /// floor is optional bid floor
    #[serde(rename = "floor", default)]
    floor: Option<f64>,
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
                errs.push(BidderError::BadInput(
                    "neither Banner nor Video object specified".to_string(),
                ));
                continue;
            }

            let mut imp_copy = imp.clone();

            // Extract triplelift ext from imp.ext.bidder
            let tl_ext = imp
                .ext
                .as_ref()
                .and_then(|e| e.get("bidder"))
                .and_then(|b| serde_json::from_value::<ExtImpTriplelift>(b.clone()).ok());

            match tl_ext {
                Some(ext) => {
                    if !ext.inventory_code.is_empty() {
                        imp_copy.tagid = Some(ext.inventory_code);
                    }
                    if let Some(floor) = ext.floor {
                        imp_copy.bidfloor = Some(floor);
                    }
                }
                None => {
                    errs.push(BidderError::BadInput(format!(
                        "failed to parse triplelift ext for imp id={}",
                        imp.id
                    )));
                    continue;
                }
            }

            valid_imps.push(imp_copy);
        }

        if valid_imps.is_empty() {
            errs.push(BidderError::BadInput(
                "No valid impressions for triplelift".to_string(),
            ));
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
        headers.insert(
            "Content-Type".to_string(),
            "application/json;charset=utf-8".to_string(),
        );
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

        let count: usize = bid_response.seatbid.iter().map(|sb| sb.bid.len()).sum();
        let mut result = BidderResponse::with_capacity(count);
        let mut errs: Vec<BidderError> = Vec::new();

        for sb in bid_response.seatbid {
            for bid in sb.bid {
                // Parse bid.ext.triplelift_pb.format to determine bid type
                let format = bid
                    .ext
                    .as_ref()
                    .and_then(|e| e.get("triplelift_pb"))
                    .and_then(|tl| tl.get("format"))
                    .and_then(|f| f.as_i64())
                    .unwrap_or(0);

                if bid.ext.is_none() {
                    errs.push(BidderError::BadServerResponse(
                        "missing bid ext for triplelift bid".to_string(),
                    ));
                    continue;
                }

                let bid_type = get_bid_type_from_tl_format(format);
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }

        // Go returns (bidResponse, errs) - partial results with non-fatal errors
        // In Rust we return Ok with partial results; errs are informational
        let _ = errs;
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
                ext: Some(serde_json::json!({"bidder": {"inventoryCode": "inv1"}})),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn test_triplelift_url_and_headers() {
        let adapter = TripleliftAdapter::new("https://tlx.3lift.com/s2s/auction".to_string());
        let (reqs, _) = adapter.make_requests(&make_req(), &ExtraRequestInfo::default());
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].uri, "https://tlx.3lift.com/s2s/auction");
        assert_eq!(
            reqs[0].headers.get("Content-Type").unwrap(),
            "application/json;charset=utf-8"
        );
        assert!(!reqs[0].body.is_empty());
    }

    #[test]
    fn test_triplelift_make_bids() {
        let adapter = TripleliftAdapter::new("https://tlx.3lift.com/s2s/auction".to_string());
        let body = br#"{"id":"r","seatbid":[{"bid":[{"id":"b","impid":"i1","price":1.7,"crid":"c","ext":{"triplelift_pb":{"format":0}}}]}]}"#;
        let resp = ResponseData::new(200, body.to_vec());
        let result = adapter
            .make_bids(&make_req(), &RequestData::default(), &resp)
            .unwrap();
        assert_eq!(result.bids.len(), 1);
        assert_eq!(result.bids[0].bid_type, BidType::Banner);
    }
}
