use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;

pub struct PulsepointAdapter {
    pub endpoint: String,
}

impl PulsepointAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

fn get_bid_type(imp: &openrtb::Imp) -> Option<BidType> {
    if imp.banner.is_some() {
        Some(BidType::Banner)
    } else if imp.video.is_some() {
        Some(BidType::Video)
    } else if imp.audio.is_some() {
        Some(BidType::Audio)
    } else if imp.native.is_some() {
        Some(BidType::Native)
    } else {
        None
    }
}

/// Parse an integer param that may be stored as number or string in JSON
fn parse_int_param(val: &serde_json::Value) -> Option<i64> {
    if let Some(n) = val.as_i64() {
        if n != 0 { return Some(n); }
    }
    if let Some(s) = val.as_str() {
        if let Ok(n) = s.parse::<i64>() {
            if n != 0 { return Some(n); }
        }
    }
    None
}

impl Bidder for PulsepointAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut imps = Vec::new();
        let mut pub_id = String::new();

        for imp in &request.imp {
            let ext = match imp.ext.as_ref() {
                Some(e) => e,
                None => {
                    errs.push(BidderError::BadInput("missing imp ext".to_string()));
                    continue;
                }
            };
            let bidder_ext = match ext.get("bidder") {
                Some(b) => b,
                None => {
                    errs.push(BidderError::BadInput("missing bidder ext".to_string()));
                    continue;
                }
            };

            // pubId
            if pub_id.is_empty() {
                let pub_val = bidder_ext.get("pubId").or_else(|| bidder_ext.get("pub_id"));
                match pub_val.and_then(|v| parse_int_param(v)) {
                    Some(n) => pub_id = n.to_string(),
                    None => {
                        errs.push(BidderError::BadInput("param not found - pubID".to_string()));
                        continue;
                    }
                }
            }

            // tagId
            let tag_val = bidder_ext.get("tagId").or_else(|| bidder_ext.get("tag_id"));
            let tag_id = match tag_val.and_then(|v| parse_int_param(v)) {
                Some(n) => n.to_string(),
                None => {
                    errs.push(BidderError::BadInput("param not found - tagID".to_string()));
                    continue;
                }
            };

            let mut imp = imp.clone();
            imp.tagid = Some(tag_id);
            imps.push(imp);
        }

        if imps.is_empty() {
            return (vec![], errs);
        }

        let mut req = request.clone();

        // Set publisher id on site or app
        let publisher = openrtb::Publisher {
            id: Some(pub_id),
            ..Default::default()
        };
        if let Some(site) = req.site.as_mut() {
            if let Some(pub_ref) = site.publisher.as_mut() {
                pub_ref.id = publisher.id;
            } else {
                site.publisher = Some(publisher);
            }
        } else if let Some(app) = req.app.as_mut() {
            if let Some(pub_ref) = app.publisher.as_mut() {
                pub_ref.id = publisher.id;
            } else {
                app.publisher = Some(publisher);
            }
        }

        req.imp = imps;

        let body = match serde_json::to_vec(&req) {
            Ok(b) => b,
            Err(e) => {
                errs.push(BidderError::BadInput(e.to_string()));
                return (vec![], errs);
            }
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
            errs,
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
        if let Err(e) = crate::check_response_status(response.status_code) {
            return Err(vec![e]);
        }

        let bid_response: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(e.to_string())])?;

        let mut result = BidderResponse::with_capacity(5);

        // Build imp map
        let imp_map: HashMap<&str, &openrtb::Imp> = internal.imp.iter()
            .map(|i| (i.id.as_str(), i))
            .collect();

        for sb in bid_response.seatbid {
            for bid in sb.bid {
                if let Some(imp) = imp_map.get(bid.impid.as_str()) {
                    if let Some(bid_type) = get_bid_type(imp) {
                        result.bids.push(TypedBid::new(bid, bid_type));
                    }
                }
            }
        }

        Ok(result)
    }
}
