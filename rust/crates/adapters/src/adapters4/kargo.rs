use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct KargoAdapter {
    pub endpoint: String,
}

impl KargoAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

#[derive(serde::Deserialize, Default)]
struct KargoExt {
    #[serde(rename = "mediaType", default)]
    media_type: String,
}

fn get_media_type_for_bid(ext: Option<&serde_json::Value>) -> BidType {
    let ext_val = match ext {
        Some(v) => v,
        None => return BidType::Banner,
    };
    if let Ok(kargo_ext) = serde_json::from_value::<KargoExt>(ext_val.clone()) {
        match kargo_ext.media_type.as_str() {
            "video" => return BidType::Video,
            "native" => return BidType::Native,
            _ => {}
        }
    }
    BidType::Banner
}

impl Bidder for KargoAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let body = match serde_json::to_vec(request) {
            Ok(b) => b,
            Err(e) => return (vec![], vec![BidderError::BadInput(e.to_string())]),
        };

        // Kargo sends the request without extra headers (no Content-Type in Go source).
        (
            vec![RequestData {
                method: "POST".to_string(),
                uri: self.endpoint.clone(),
                body,
                headers: HashMap::new(),
                imp_ids: get_imp_ids(&request.imp),
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
                "Unexpected status code: {}. Run with request.debug = 1 for more info.",
                response.status_code
            ))]);
        }

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(5);
        if let Some(cur) = &bid_resp.cur {
            result.currency = cur.clone();
        }

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = get_media_type_for_bid(bid.ext.as_ref());
                result.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_make_requests_basic() {
        let adapter = KargoAdapter::new("https://krushmedia.example/rtb".to_string());
        let mut req = openrtb::BidRequest::default();
        req.id = "r".to_string();
        req.imp = vec![openrtb::Imp {
            id: "imp1".to_string(),
            banner: Some(Default::default()),
            ..Default::default()
        }];
        let info = ExtraRequestInfo::default();
        let (requests, errs) = adapter.make_requests(&req, &info);
        assert!(errs.is_empty());
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].method, "POST");
        assert!(requests[0].headers.is_empty());
    }

    #[test]
    fn test_media_type_video() {
        let ext = serde_json::json!({"mediaType": "video"});
        assert_eq!(get_media_type_for_bid(Some(&ext)), BidType::Video);
    }
}
