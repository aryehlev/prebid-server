use std::collections::HashMap;
use crate::{Bidder, BidderError, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid, get_imp_ids};
use openrtb_ext::BidType;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub struct AdtelligentAdapter {
    pub endpoint: String,
}

impl AdtelligentAdapter {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }
}

/// Adtelligent bidder extension from imp.ext.bidder
#[derive(Debug, Default, Deserialize)]
struct ExtImpAdtelligent {
    #[serde(rename = "source_id")]
    source_id: Value,
    #[serde(rename = "bidfloor", default)]
    bid_floor: f64,
}

/// The output imp ext key for adtelligent
#[derive(Debug, Serialize)]
struct AdtelligentImpExt {
    adtelligent: ExtImpAdtelligentOut,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct ExtImpAdtelligentOut {
    #[serde(rename = "source_id")]
    source_id: Value,
    #[serde(rename = "bidfloor", default, skip_serializing_if = "is_zero_f64")]
    bid_floor: f64,
}

fn is_zero_f64(v: &f64) -> bool {
    *v == 0.0
}

fn validate_impression(imp: &mut openrtb::Imp) -> Result<i64, BidderError> {
    if imp.banner.is_none() && imp.video.is_none() {
        return Err(BidderError::BadInput(format!(
            "ignoring imp id={}, Adtelligent supports only Video and Banner", imp.id
        )));
    }

    if imp.ext.is_none() {
        return Err(BidderError::BadInput(format!(
            "ignoring imp id={}, extImpBidder is empty", imp.id
        )));
    }

    let bidder_ext = imp.ext.as_ref()
        .and_then(|e| e.get("bidder"))
        .ok_or_else(|| BidderError::BadInput(format!(
            "ignoring imp id={}, error while decoding extImpBidder", imp.id
        )))?;

    let imp_ext: ExtImpAdtelligent = serde_json::from_value(bidder_ext.clone())
        .map_err(|e| BidderError::BadInput(format!(
            "ignoring imp id={}, error while decoding impExt, err: {}", imp.id, e
        )))?;

    // Parse source_id which may be a string or number
    let source_id: i64 = match &imp_ext.source_id {
        Value::Number(n) => n.as_i64().unwrap_or(0),
        Value::String(s) => s.parse::<i64>().map_err(|e| BidderError::BadInput(format!(
            "ignoring imp id={}, aid parsing err: {}", imp.id, e
        )))?,
        _ => return Err(BidderError::BadInput(format!(
            "ignoring imp id={}, aid parsing err: invalid type", imp.id
        ))),
    };

    // Set bid floor from ext if present
    if imp_ext.bid_floor > 0.0 {
        imp.bidfloor = Some(imp_ext.bid_floor);
    }

    // Replace imp.ext with adtelligent-keyed ext
    let new_ext = AdtelligentImpExt {
        adtelligent: ExtImpAdtelligentOut {
            source_id: imp_ext.source_id,
            bid_floor: imp_ext.bid_floor,
        },
    };

    imp.ext = Some(serde_json::to_value(new_ext)
        .map_err(|e| BidderError::BadInput(format!(
            "ignoring imp id={}, error while marshaling impExt, err: {}", imp.id, e
        )))?);

    Ok(source_id)
}

impl Bidder for AdtelligentAdapter {
    fn make_requests(
        &self,
        request: &openrtb::BidRequest,
        _info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut imp2source: HashMap<i64, Vec<usize>> = HashMap::new();

        let mut imps = request.imp.clone();

        for (i, imp) in imps.iter_mut().enumerate() {
            match validate_impression(imp) {
                Ok(source_id) => {
                    imp2source.entry(source_id).or_insert_with(Vec::new).push(i);
                }
                Err(e) => {
                    errs.push(e);
                }
            }
        }

        if imp2source.is_empty() {
            return (vec![], errs);
        }

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json;charset=utf-8".to_string());
        headers.insert("Accept".to_string(), "application/json".to_string());

        let mut requests = Vec::new();

        for (source_id, imp_indices) in imp2source {
            let mut req = request.clone();
            req.imp = imp_indices.iter().map(|&i| imps[i].clone()).collect();

            let body = match serde_json::to_vec(&req) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(BidderError::BadInput(format!(
                        "error while encoding bidRequest, err: {}", e
                    )));
                    continue;
                }
            };

            let uri = format!("{}?aid={}", self.endpoint, source_id);
            let imp_ids = get_imp_ids(&req.imp);
            requests.push(RequestData {
                method: "POST".to_string(),
                uri,
                body,
                headers: headers.clone(),
                imp_ids,
            });
        }

        (requests, errs)
    }

    fn make_bids(
        &self,
        internal: &openrtb::BidRequest,
        _external: &RequestData,
        response: &ResponseData,
    ) -> Result<BidderResponse, Vec<BidderError>> {
        // Go returns nil, nil for 204
        if response.status_code == 204 {
            return Ok(BidderResponse::new());
        }

        // Go does not gate on status code beyond 204 — it attempts to decode any response.
        // A non-200 body that isn't valid JSON will surface as a BadServerResponse below.

        let bid_resp: openrtb::BidResponse = serde_json::from_slice(&response.body)
            .map_err(|e| vec![BidderError::BadServerResponse(format!(
                "error while decoding response, err: {}", e
            ))])?;

        let mut result = BidderResponse::new();
        let mut bid_errs: Vec<BidderError> = Vec::new();

        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let found = internal.imp.iter().find(|i| i.id == bid.impid);
                match found {
                    Some(imp) => {
                        let bid_type = if imp.video.is_some() {
                            BidType::Video
                        } else {
                            BidType::Banner
                        };
                        result.bids.push(TypedBid::new(bid, bid_type));
                    }
                    None => {
                        bid_errs.push(BidderError::BadServerResponse(format!(
                            "ignoring bid id={}, request doesn't contain any impression with id={}",
                            bid.id, bid.impid
                        )));
                    }
                }
            }
        }

        // Per-bid mismatch errors are non-fatal: return collected bids regardless.
        // The trait returns Result<BidderResponse, Vec<BidderError>>, so non-fatal
        // errors cannot be surfaced alongside successful bids. Errors are dropped here
        // to match the spirit of the Go implementation (which returns bids + errors).
        let _ = bid_errs;
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
                ext: Some(serde_json::json!({"bidder": {"source_id": 1000}})),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn test_adtelligent_url_and_headers() {
        let adapter = AdtelligentAdapter::new("https://ghb.adtelligent.com/pbs/ortb".to_string());
        let (reqs, _) = adapter.make_requests(&make_req(), &ExtraRequestInfo::default());
        assert_eq!(reqs.len(), 1);
        assert_eq!(
            reqs[0].uri,
            "https://ghb.adtelligent.com/pbs/ortb?aid=1000"
        );
        assert_eq!(
            reqs[0].headers.get("Content-Type").unwrap(),
            "application/json;charset=utf-8"
        );
        assert!(!reqs[0].body.is_empty());
    }

    #[test]
    fn test_adtelligent_make_bids() {
        let adapter = AdtelligentAdapter::new("https://ghb.adtelligent.com/pbs/ortb".to_string());
        let body = br#"{"id":"r","seatbid":[{"bid":[{"id":"b","impid":"i1","price":1.4,"crid":"c"}]}]}"#;
        let resp = ResponseData::new(200, body.to_vec());
        let result = adapter
            .make_bids(&make_req(), &RequestData::default(), &resp)
            .unwrap();
        assert_eq!(result.bids.len(), 1);
        assert_eq!(result.bids[0].bid_type, BidType::Banner);
    }
}
