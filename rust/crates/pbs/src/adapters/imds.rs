//! Go `adapters/imds/imds.go`.

use serde::Deserialize;

use crate::bid_types::BidType;
use crate::bidder::{Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::macros::{EndpointTemplate, EndpointTemplateParams};
use crate::ortb::openrtb2::{BidRequest, BidResponse, Imp};
use crate::ortb::Ext;

const ADAPTER_VERSION: &str = "pbs-go/1.0.0";

pub struct Adapter {
    endpoint_template: EndpointTemplate,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: &str) -> Result<Self, String> {
        let endpoint_template = EndpointTemplate::parse(endpoint)
            .map_err(|e| format!("unable to parse endpoint url template: {e}"))?;
        Ok(Self { endpoint_template })
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpBidder {
    bidder: Option<Ext>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpImds {
    #[serde(rename = "seatId")]
    seat_id: String,
    #[serde(rename = "tagId")]
    tag_id: String,
}

fn ext_text(ext: &Option<Ext>) -> Vec<u8> {
    ext.as_ref().map(|e| e.to_json().into_bytes()).unwrap_or_default()
}

/// Go's `url.QueryEscape`.
fn query_escape(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// json-iterator names the Go field path when a string field gets another JSON type; serde's
/// wording is replaced for the two fields of `ExtImpImds`.
fn imds_type_error(data: &[u8]) -> Option<String> {
    let v: serde_json::Value = serde_json::from_slice(data).ok()?;
    for (key, field) in [("seatId", "SeatId"), ("tagId", "TagId")] {
        if let Some(val) = v.get(key) {
            if !val.is_string() && !val.is_null() {
                let text = val.to_string();
                return Some(format!(
                    "cannot unmarshal openrtb_ext.ExtImpImds.{field}: expects \" or n, but found {}",
                    text.chars().next().unwrap_or(' ')
                ));
            }
        }
    }
    None
}

fn get_ext_imp_obj(imp: &Imp) -> Result<ExtImpImds, BidderError> {
    let bidder_ext: ExtImpBidder =
        jsonutil::unmarshal(&ext_text(&imp.ext)).map_err(|e| BidderError::bad_input(e.to_string()))?;
    let bytes = ext_text(&bidder_ext.bidder);
    jsonutil::unmarshal(&bytes).map_err(|e| {
        BidderError::bad_input(imds_type_error(&bytes).unwrap_or_else(|| e.to_string()))
    })
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = vec![];
        let mut valid_imps: Vec<Imp> = vec![];
        let mut first_ext_imp: Option<ExtImpImds> = None;
        for imp in &request.imp {
            let valid = match get_ext_imp_obj(imp) {
                Ok(v) => v,
                Err(e) => {
                    errs.push(e);
                    continue;
                }
            };
            if valid.seat_id.is_empty() || valid.tag_id.is_empty() {
                errs.push(BidderError::bad_server_response("Invalid Impression"));
                continue;
            }
            let mut imp = imp.clone();
            imp.tagid = valid.tag_id.clone();
            valid_imps.push(imp);
            if first_ext_imp.is_none() {
                first_ext_imp = Some(valid);
            }
        }
        if valid_imps.is_empty() {
            return (vec![], errs);
        }
        let Some(first) = first_ext_imp.filter(|f| !f.seat_id.is_empty() && !f.tag_id.is_empty()) else {
            errs.push(BidderError::bad_server_response("Invalid Impression"));
            return (vec![], errs);
        };

        let mut req = request.clone();
        req.imp = valid_imps;
        req.ext = match Ext::from_serialize(&serde_json::json!({ "seatId": first.seat_id })) {
            Ok(e) => Some(e),
            Err(e) => {
                errs.push(BidderError::other(e.to_string()));
                return (vec![], errs);
            }
        };
        let body = match crate::go_json::to_vec(&req) {
            Ok(b) => b,
            Err(e) => {
                errs.push(BidderError::other(e.to_string()));
                return (vec![], errs);
            }
        };
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        let params = EndpointTemplateParams {
            account_id: query_escape(&first.seat_id),
            source_id: query_escape(ADAPTER_VERSION),
            ..Default::default()
        };
        let uri = match self.endpoint_template.resolve(&params) {
            Ok(u) => u,
            Err(e) => {
                errs.push(BidderError::other(e));
                return (vec![], errs);
            }
        };
        (
            vec![RequestData {
                method: "POST".into(),
                uri,
                body,
                headers,
                imp_ids: req.imp.iter().map(|i| i.id.clone()).collect(),
            }],
            errs,
        )
    }

    fn make_bids(
        &self,
        request: &BidRequest,
        _request_data: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        let msg = || {
            format!(
                "Unexpected status code: {}. Run with request.debug = 1 for more info",
                response.status_code
            )
        };
        match response.status_code {
            204 => return (None, vec![]),
            400 => return (None, vec![BidderError::bad_input(msg())]),
            200 => {}
            _ => return (None, vec![BidderError::bad_server_response(msg())]),
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut bid_response = BidderResponse::with_bids_capacity(1);
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let media_type = get_media_type_for_imp(&bid.impid, &request.imp);
                if media_type != BidType::Banner && media_type != BidType::Video {
                    continue;
                }
                bid_response.bids.push(TypedBid::new(bid, media_type));
            }
        }
        (Some(bid_response), vec![])
    }
}

fn get_media_type_for_imp(imp_id: &str, imps: &[Imp]) -> BidType {
    let mut media_type = BidType::Banner;
    for imp in imps {
        if imp.id == imp_id {
            if imp.banner.is_some() {
                break;
            }
            if imp.video.is_some() {
                media_type = BidType::Video;
                break;
            }
            if imp.native.is_some() {
                media_type = BidType::Native;
                break;
            }
            if imp.audio.is_some() {
                media_type = BidType::Audio;
                break;
            }
        }
    }
    media_type
}
