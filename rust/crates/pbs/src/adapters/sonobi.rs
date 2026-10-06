//! Go `adapters/sonobi/sonobi.go`.

use serde::Deserialize;

use crate::bid_types::BidType;
use crate::bidder::{
    Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{BidRequest, BidResponse, Imp};
use crate::ortb::Ext;

pub struct Adapter {
    uri: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpBidder {
    bidder: Option<Ext>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpSonobi {
    tagid: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Result<Self, BidderError> {
        Ok(Self { uri: endpoint.into() })
    }
}

fn unm(e: impl ToString) -> BidderError {
    BidderError::FailedToUnmarshal(e.to_string())
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::new();
        let mut out = Vec::new();

        for imp in &request.imp {
            let mut req_copy = request.clone();
            req_copy.imp = vec![imp.clone()];

            let bidder_ext: ExtImpBidder = match imp.ext.as_ref().map(decode_ci) {
                Some(Ok(v)) => v,
                Some(Err(e)) => {
                    errs.push(e);
                    continue;
                }
                None => {
                    errs.push(unm("unexpected end of JSON input"));
                    continue;
                }
            };
            let sonobi: ExtImpSonobi = match bidder_ext.bidder.as_ref().map(decode_ci) {
                Some(Ok(v)) => v,
                Some(Err(e)) => {
                    errs.push(e);
                    continue;
                }
                None => {
                    errs.push(unm("unexpected end of JSON input"));
                    continue;
                }
            };

            let i0 = &mut req_copy.imp[0];
            i0.tagid = sonobi.tagid;

            if i0.bidfloor > 0.0
                && !i0.bidfloorcur.is_empty()
                && i0.bidfloorcur.to_uppercase() != "USD"
            {
                match req_info.convert_currency(i0.bidfloor, &i0.bidfloorcur, "USD") {
                    Ok(v) => {
                        i0.bidfloorcur = "USD".into();
                        i0.bidfloor = v;
                    }
                    Err(e) => {
                        errs.push(e);
                        continue;
                    }
                }
            }

            req_copy.cur = vec!["USD".into()];

            match crate::go_json::to_vec(&req_copy) {
                Ok(body) => {
                    let mut headers = Header::new();
                    headers.add("Content-Type", "application/json;charset=utf-8");
                    headers.add("Accept", "application/json");
                    out.push(RequestData {
                        method: "POST".into(),
                        uri: self.uri.clone(),
                        body,
                        headers,
                        imp_ids: req_copy.imp.iter().map(|i| i.id.clone()).collect(),
                    });
                }
                Err(e) => errs.push(BidderError::other(e.to_string())),
            }
        }
        (out, errs)
    }

    fn make_bids(
        &self,
        internal_request: &BidRequest,
        _request_data: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if response.status_code == 204 {
            return (None, vec![]);
        }
        if response.status_code == 400 {
            return (
                None,
                vec![BidderError::bad_input(format!(
                    "Unexpected status code: {}. Run with request.debug = 1 for more info",
                    response.status_code
                ))],
            );
        }
        if response.status_code != 200 {
            return (
                None,
                vec![BidderError::bad_server_response(format!(
                    "Unexpected status code: {}. Run with request.debug = 1 for more info",
                    response.status_code
                ))],
            );
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut bid_response = BidderResponse::with_bids_capacity(5);
        bid_response.currency = "USD".into();
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let bid_type = match get_media_type_for_imp(&bid.impid, &internal_request.imp) {
                    Ok(t) => t,
                    Err(e) => return (None, vec![e]),
                };
                bid_response.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        (Some(bid_response), vec![])
    }
}

fn get_media_type_for_imp(imp_id: &str, imps: &[Imp]) -> Result<BidType, BidderError> {
    let mut media_type = BidType::Banner;
    for imp in imps {
        if imp.id == imp_id {
            if imp.banner.is_none() && imp.video.is_some() {
                media_type = BidType::Video;
            }
            if imp.banner.is_none() && imp.video.is_none() && imp.native.is_some() {
                media_type = BidType::Native;
            }
            return Ok(media_type);
        }
    }
    Err(BidderError::bad_input(format!("Failed to find impression \"{imp_id}\" ")))
}

// ---- local helper: Go `jsonutil.Unmarshal` on an ext (json-iterator matches keys case-insensitively).
fn decode_ci<T: serde::de::DeserializeOwned>(ext: &crate::ortb::Ext) -> Result<T, BidderError> {
    let text = ext.to_json();
    if let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(&text) {
        let lowered: serde_json::Map<String, serde_json::Value> =
            map.into_iter().map(|(k, v)| (k.to_lowercase(), v)).collect();
        return serde_json::from_value(serde_json::Value::Object(lowered))
            .map_err(|e| BidderError::FailedToUnmarshal(e.to_string()));
    }
    jsonutil::unmarshal(text.as_bytes())
}
