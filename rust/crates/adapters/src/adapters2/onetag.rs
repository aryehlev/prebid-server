use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct OnetagAdapter {
    pub endpoint: String,
}

impl OnetagAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }

    fn build_endpoint_url(&self, pub_id: &str) -> String {
        // Replace {{.PublisherID}} macro
        self.endpoint.replace("{{.PublisherID}}", pub_id)
    }
}

fn get_media_type_for_bid(imp_id: &str, imps: &[openrtb::Imp]) -> Result<BidType, BidderError> {
    for imp in imps {
        if imp.id == imp_id {
            if imp.banner.is_some() {
                return Ok(BidType::Banner);
            }
            if imp.video.is_some() {
                return Ok(BidType::Video);
            }
            if imp.native.is_some() {
                return Ok(BidType::Native);
            }
        }
    }
    Err(BidderError::BadServerResponse(format!(
        "The impression with ID {} is not present into the request", imp_id
    )))
}

impl Bidder for OnetagAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut pub_id = String::new();

        for imp in &request.imp {
            let onetag_ext = match imp.ext.as_ref()
                .and_then(|e| e.get("bidder"))
            {
                Some(b) => b.clone(),
                None => return (vec![], vec![BidderError::BadInput(
                    "Bidder extension not provided or can't be unmarshalled".to_string()
                )]),
            };

            let pid = onetag_ext.get("pubId")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            if pid.is_empty() {
                return (vec![], vec![BidderError::BadInput(
                    "The publisher ID must not be empty".to_string()
                )]);
            }

            if pub_id.is_empty() {
                pub_id = pid.to_string();
            } else if pub_id != pid {
                return (vec![], vec![BidderError::BadInput(
                    "There must be only one publisher ID".to_string()
                )]);
            }
        }

        let url = self.build_endpoint_url(&pub_id);

        let body = match serde_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        let imp_ids = get_imp_ids(&request.imp);
        let mut headers = HashMap::new();
        headers.insert(
            "Content-Type".to_string(),
            "application/json;charset=utf-8".to_string(),
        );
        headers.insert("Accept".to_string(), "application/json".to_string());
        (
            vec![RequestData {
                method: "POST".to_string(),
                uri: url,
                body,
                headers,
                imp_ids,
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
        if response.status_code != 200 {
            return Err(vec![BidderError::BadServerResponse(format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info.", response.status_code
            ))]);
        }

        let bid_response: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(internal.imp.len());
        if let Some(cur) = bid_response.cur.as_deref() {
            if !cur.is_empty() {
                result.currency = cur.to_string();
            }
        }

        for sb in bid_response.seatbid {
            for bid in sb.bid {
                let bid_type = get_media_type_for_bid(&bid.impid, &internal.imp)
                    .map_err(|e| vec![e])?;
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
                ext: Some(serde_json::json!({"bidder": {"pubId": "publisherId"}})),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn test_onetag_url_and_headers() {
        let adapter = OnetagAdapter::new("https://onetag-sys.com/prebid-request/{{.PublisherID}}".to_string());
        let (reqs, _) = adapter.make_requests(&make_req(), &ExtraRequestInfo::default());
        assert_eq!(reqs.len(), 1);
        assert_eq!(
            reqs[0].uri,
            "https://onetag-sys.com/prebid-request/publisherId"
        );
        assert_eq!(
            reqs[0].headers.get("Content-Type").unwrap(),
            "application/json;charset=utf-8"
        );
        assert!(!reqs[0].body.is_empty());
    }

    #[test]
    fn test_onetag_make_bids() {
        let adapter = OnetagAdapter::new("https://onetag-sys.com/prebid-request/{{.PublisherID}}".to_string());
        let body = br#"{"id":"r","seatbid":[{"bid":[{"id":"b","impid":"i1","price":1.8,"crid":"c"}]}]}"#;
        let resp = ResponseData::new(200, body.to_vec());
        let result = adapter
            .make_bids(&make_req(), &RequestData::default(), &resp)
            .unwrap();
        assert_eq!(result.bids.len(), 1);
        assert_eq!(result.bids[0].bid_type, BidType::Banner);
    }
}
