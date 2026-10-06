//! Go `adapters/aja/aja.go`.

#![allow(unused_imports, dead_code)]
use crate::bid_types::BidType;
use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder, BidderResponse,
    ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::macros::{EndpointTemplate, EndpointTemplateParams};
use crate::ortb::openrtb2::{Bid, BidRequest, BidResponse, Imp, MarkupType};
use crate::ortb::Ext;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Default)]
struct ExtImpAja {
    #[serde(rename = "asi", default)]
    ad_spot_id: String,
}

pub struct Adapter {
    endpoint: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self { endpoint: endpoint.into() }
    }
}

fn parse_ext_aja(imp: &Imp) -> Result<ExtImpAja, BidderError> {
    let bidder = imp_bidder_raw(&imp.ext).map_err(|e| {
        BidderError::bad_input(format!("Failed to unmarshal ext impID: {} err: {}", imp.id, e))
    })?;
    check_string_fields(&bidder, "openrtb_ext.ExtImpAJA", &[("asi", "AdSpotID")])
        .and_then(|_| unmarshal_raw::<ExtImpAja>(&bidder))
        .map_err(|e| BidderError::bad_input(format!("Failed to unmarshal ext.bidder impID: {} err: {}", imp.id, e)))
}

impl Bidder for Adapter {
    fn make_requests(&self, request: &BidRequest, _req_info: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        // split imps by tagid
        let mut errs = vec![];
        let mut tag_ids: Vec<String> = vec![];
        let mut imps_by_tag: std::collections::HashMap<String, Vec<Imp>> = std::collections::HashMap::new();
        for imp in &request.imp {
            let ext = match parse_ext_aja(imp) {
                Ok(e) => e,
                Err(e) => {
                    errs.push(e);
                    continue;
                }
            };
            let mut imp = imp.clone();
            imp.tagid = ext.ad_spot_id;
            imp.ext = None;
            if !imps_by_tag.contains_key(&imp.tagid) {
                tag_ids.push(imp.tagid.clone());
            }
            imps_by_tag.entry(imp.tagid.clone()).or_default().push(imp);
        }

        let (base, _) = split_request(request);
        let mut reqs = vec![];
        for tag_id in tag_ids {
            let mut req = base.clone();
            req.imp = imps_by_tag.remove(&tag_id).unwrap_or_default();
            let body = match marshal(&req) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(BidderError::bad_input(format!(
                        "Failed to unmarshal bidrequest ID: {} err: {}",
                        request.id, e
                    )));
                    continue;
                }
            };
            reqs.push(RequestData {
                method: "POST".into(),
                uri: self.endpoint.clone(),
                body,
                headers: Header::new(),
                imp_ids: imp_ids(&req.imp),
            });
        }
        (reqs, errs)
    }

    fn make_bids(
        &self,
        request: &BidRequest,
        _request_data: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if response.status_code != 200 {
            if response.status_code == 204 {
                return (None, vec![]);
            }
            let msg = format!("Unexpected status code: {}", response.status_code);
            if response.status_code == 400 {
                return (None, vec![BidderError::bad_input(msg)]);
            }
            return (None, vec![BidderError::bad_server_response(msg)]);
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(e) => {
                return (None, vec![BidderError::bad_server_response(format!("Failed to unmarshal bid response: {e}"))])
            }
        };
        let mut bidder_resp = BidderResponse::with_bids_capacity(request.imp.len());
        let mut errors = vec![];
        for seatbid in bid_resp.seatbid {
            for bid in seatbid.bid {
                let Some(imp) = request.imp.iter().find(|imp| imp.id == bid.impid) else {
                    continue;
                };
                let bid_type = if imp.banner.is_some() {
                    BidType::Banner
                } else if imp.video.is_some() {
                    BidType::Video
                } else {
                    errors.push(BidderError::bad_server_response(format!(
                        "Response received for unexpected type of bid bidID: {}",
                        bid.id
                    )));
                    continue;
                };
                bidder_resp.bids.push(TypedBid::new(bid, bid_type));
            }
        }
        if !bid_resp.cur.is_empty() {
            bidder_resp.currency = bid_resp.cur;
        }
        (Some(bidder_resp), errors)
    }
}

// ---- local helpers (Go `adapters.ExtImpBidder` + `jsonutil.Unmarshal` on raw ext bytes) ----

#[derive(Debug, Default, serde::Deserialize)]
struct ExtImpBidder {
    #[serde(default)]
    bidder: Option<Ext>,
}

/// Go `jsonutil.Unmarshal(raw, &v)` where `raw` may be empty (nil `json.RawMessage`): json-iterator
/// reports the NUL it reads past the end of the input. `null` leaves `v` at its zero value.
fn unmarshal_raw<T: serde::de::DeserializeOwned + Default>(raw: &[u8]) -> Result<T, BidderError> {
    if raw.is_empty() {
        return Err(BidderError::FailedToUnmarshal("expect { or n, but found \u{0}".to_string()));
    }
    if raw.iter().find(|b| !b" \t\r\n".contains(b)) == Some(&b'n') && raw.trim_ascii() == b"null" {
        return Ok(T::default());
    }
    jsonutil::unmarshal(raw)
}

fn ext_bytes(ext: &Option<Ext>) -> Vec<u8> {
    ext.as_ref().map(|e| e.to_json().into_bytes()).unwrap_or_default()
}

/// Go `jsonutil.Unmarshal(imp.Ext, &adapters.ExtImpBidder)`, returning `bidderExt.Bidder` bytes
/// (empty when absent).
fn imp_bidder_raw(ext: &Option<Ext>) -> Result<Vec<u8>, BidderError> {
    let parsed: ExtImpBidder = unmarshal_raw(&ext_bytes(ext))?;
    Ok(parsed.bidder.map(|b| b.to_json().into_bytes()).unwrap_or_default())
}

/// Go `json.Marshal(v)` into a `json.RawMessage` stand-in.
fn ext_from<T: serde::Serialize>(v: &T) -> Result<Ext, BidderError> {
    let bytes = crate::go_json::to_vec(v).map_err(|e| BidderError::other(e.to_string()))?;
    Ext::from_slice(&bytes).map_err(|e| BidderError::other(e.to_string()))
}

/// Go `openrtb_ext.GetImpIDs`.
fn imp_ids(imps: &[Imp]) -> Vec<String> {
    imps.iter().map(|i| i.id.clone()).collect()
}

/// Go `reqCopy := *request` then replacing `Imp`: a copy of the request without its imps, plus the
/// imps (cloned once).
fn split_request(request: &BidRequest) -> (BidRequest, Vec<Imp>) {
    let mut base = request.clone();
    let imps = std::mem::take(&mut base.imp);
    (base, imps)
}

/// Go `url.QueryEscape`.
fn query_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn marshal(request: &BidRequest) -> Result<Vec<u8>, BidderError> {
    crate::go_json::to_vec(request).map_err(|e| BidderError::other(e.to_string()))
}

/// jsoniter's struct-path message for a JSON string field holding a non-string value, e.g.
/// `cannot unmarshal openrtb_ext.ExtImpAJA.AdSpotID: expects " or n, but found 1`. Checked before
/// the serde decode, which words it differently. `fields` is `(json key, Go field name)`.
fn check_string_fields(raw: &[u8], go_struct: &str, fields: &[(&str, &str)]) -> Result<(), BidderError> {
    let Ok(serde_json::Value::Object(obj)) = serde_json::from_slice::<serde_json::Value>(raw) else {
        return Ok(());
    };
    for (key, go_field) in fields {
        if let Some(v) = obj.get(*key) {
            let found = match v {
                serde_json::Value::Null | serde_json::Value::String(_) => continue,
                serde_json::Value::Array(_) => '[',
                serde_json::Value::Object(_) => '{',
                serde_json::Value::Bool(true) => 't',
                serde_json::Value::Bool(false) => 'f',
                serde_json::Value::Number(n) => n.to_string().chars().next().unwrap_or('0'),
            };
            return Err(BidderError::FailedToUnmarshal(format!(
                "cannot unmarshal {go_struct}.{go_field}: expects \" or n, but found {found}"
            )));
        }
    }
    Ok(())
}
