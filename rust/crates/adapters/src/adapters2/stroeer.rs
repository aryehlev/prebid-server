use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::Deserialize;

pub struct StroeerAdapter {
    pub endpoint: String,
}

impl StroeerAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

/// Stroeer's custom response format (non-OpenRTB)
#[derive(Debug, Deserialize)]
struct StroeerResponse {
    #[serde(default)]
    bids: Vec<StroeerBid>,
}

#[derive(Debug, Deserialize)]
struct StroeerBid {
    id: String,
    #[serde(rename = "bidId")]
    bid_id: String,
    cpm: f64,
    width: i32,
    height: i32,
    ad: String,
    crid: String,
    mtype: String,
    #[serde(default)]
    adomain: Vec<String>,
    ext: Option<serde_json::Value>,
    dsa: Option<serde_json::Value>,
}

impl StroeerBid {
    fn resolve_media_type(&self) -> Result<BidType, BidderError> {
        match self.mtype.as_str() {
            "banner" => Ok(BidType::Banner),
            "video" => Ok(BidType::Video),
            _ => Err(BidderError::BadServerResponse(format!(
                "unable to determine media type for bid with id \"{}\"",
                self.bid_id
            ))),
        }
    }

    fn markup_type(&self) -> Option<i32> {
        match self.mtype.as_str() {
            "banner" => Some(1), // MarkupBanner
            "video" => Some(2),  // MarkupVideo
            _ => None,
        }
    }

    fn get_bid_ext(&self) -> Option<serde_json::Value> {
        match &self.dsa {
            None => self.ext.clone(),
            Some(dsa) => {
                let mut ext_map: serde_json::Map<String, serde_json::Value> =
                    serde_json::Map::new();
                if let Some(ext) = &self.ext {
                    if let serde_json::Value::Object(m) = ext {
                        ext_map = m.clone();
                    }
                }
                ext_map.insert("dsa".to_string(), dsa.clone());
                Some(serde_json::Value::Object(ext_map))
            }
        }
    }
}

impl Bidder for StroeerAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errors = Vec::new();
        let mut req = request.clone();

        // Extract sid from bidder ext and set as tagid
        for imp in req.imp.iter_mut() {
            let sid = imp
                .ext
                .as_ref()
                .and_then(|e| e.get("bidder"))
                .and_then(|b| b.get("sid"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            if let Some(s) = sid {
                imp.tagid = Some(s);
            }
        }

        let body = match serde_json::to_vec(&req) {
            Ok(b) => b,
            Err(e) => {
                errors.push(BidderError::BadInput(e.to_string()));
                return (vec![], errors);
            }
        };

        let mut headers = HashMap::new();
        headers.insert(
            "Content-Type".to_string(),
            "application/json;charset=utf-8".to_string(),
        );
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
            errors,
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
                "Unexpected http status code: {}.",
                response.status_code
            ))]);
        }

        let stroeer_resp: StroeerResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(stroeer_resp.bids.len());
        result.currency = "EUR".to_string();

        let mut errors = Vec::new();
        for bid_data in stroeer_resp.bids {
            let bid_type = match bid_data.resolve_media_type() {
                Ok(t) => t,
                Err(e) => {
                    errors.push(BidderError::BadServerResponse(format!(
                        "Bid media type error: {}",
                        match e {
                            BidderError::BadServerResponse(msg) => msg,
                            other => format!("{:?}", other),
                        }
                    )));
                    continue;
                }
            };

            let markup_type = bid_data.markup_type();
            let ext = bid_data.get_bid_ext();

            let bid = openrtb::Bid {
                id: bid_data.id.clone(),
                impid: bid_data.bid_id.clone(),
                w: Some(bid_data.width),
                h: Some(bid_data.height),
                price: bid_data.cpm,
                adm: Some(bid_data.ad.clone()),
                crid: Some(bid_data.crid.clone()),
                adomain: if bid_data.adomain.is_empty() {
                    None
                } else {
                    Some(bid_data.adomain.clone())
                },
                ext,
                mtype: markup_type,
                ..Default::default()
            };

            result.bids.push(TypedBid::new(bid, bid_type));
        }

        if errors.is_empty() {
            Ok(result)
        } else {
            // Return partial results - errors are non-fatal per Go behavior
            Ok(result)
        }
    }
}
