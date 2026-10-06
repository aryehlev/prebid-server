//! Go `adapters/adview/adview.go`.

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
struct ExtImpAdView {
    #[serde(rename = "placementId", default)]
    master_tag_id: String,
    #[serde(rename = "accountId", default)]
    account_id: String,
}

pub struct Adapter {
    endpoint: EndpointTemplate,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl AsRef<str>) -> Result<Self, BidderError> {
        let endpoint = parse_template(endpoint.as_ref())
            .map_err(|e| BidderError::other(format!("unable to parse endpoint url template: {e}")))?;
        Ok(Self { endpoint })
    }
}

impl Bidder for Adapter {
    fn make_requests(&self, request: &BidRequest, req_info: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut requests = vec![];
        let mut errors = vec![];
        let (base, imps) = split_request(request);
        // Go keeps one request copy across the loop, so `Cur` set by an earlier imp sticks.
        let mut request_copy = base;
        for imp in imps {
            let bidder = match imp_bidder_raw(&imp.ext) {
                Ok(b) => b,
                Err(e) => {
                    errors.push(BidderError::bad_input(format!("invalid imp.ext, {e}")));
                    continue;
                }
            };
            let adv: ExtImpAdView = match unmarshal_raw(&bidder) {
                Ok(a) => a,
                Err(e) => {
                    errors.push(BidderError::bad_input(format!("invalid bidderExt.Bidder, {e}")));
                    continue;
                }
            };
            let mut imp = imp;
            imp.tagid = adv.master_tag_id.clone(); // tagid means posid
            if let Some(banner) = imp.banner.as_mut() {
                if let Some(first) = banner.format.first() {
                    banner.h = Some(first.h);
                    banner.w = Some(first.w);
                }
            }
            // Check if imp comes with bid floor amount defined in a foreign currency
            if imp.bidfloor > 0.0 && !imp.bidfloorcur.is_empty() && imp.bidfloorcur.to_uppercase() != "USD" {
                match req_info.convert_currency(imp.bidfloor, &imp.bidfloorcur, "USD") {
                    Ok(v) => {
                        imp.bidfloorcur = "USD".into();
                        imp.bidfloor = v;
                    }
                    Err(e) => {
                        errors.push(e);
                        continue;
                    }
                }
            }
            // Set the CUR of bid to USD after converting all floors
            request_copy.cur = vec!["USD".into()];
            request_copy.imp = vec![imp];

            let url = match self
                .endpoint
                .resolve(&EndpointTemplateParams { account_id: adv.account_id, ..Default::default() })
            {
                Ok(u) => u,
                Err(e) => {
                    errors.push(BidderError::other(e));
                    continue;
                }
            };
            let body = match marshal(&request_copy) {
                Ok(b) => b,
                Err(e) => {
                    errors.push(e);
                    continue;
                }
            };
            requests.push(RequestData {
                method: "POST".into(),
                uri: url,
                body,
                headers: Header::new(),
                imp_ids: imp_ids(&request_copy.imp),
            });
        }
        (requests, errors)
    }

    fn make_bids(
        &self,
        request: &BidRequest,
        _request_data: &RequestData,
        response_data: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        match response_data.status_code {
            204 => return (None, vec![]),
            400 => {
                return (
                    None,
                    vec![BidderError::bad_input("Unexpected status code: 400. Bad request from publisher.")],
                )
            }
            200 => {}
            c => return (None, vec![BidderError::bad_server_response(format!("Unexpected status code: {c}."))]),
        }
        let response: BidResponse = match jsonutil::unmarshal(&response_data.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut bid_response = BidderResponse::with_bids_capacity(request.imp.len());
        // we just support USD for resp
        bid_response.currency = "USD".into();
        let mut errors = vec![];
        for seat_bid in response.seatbid {
            for bid in seat_bid.bid {
                let t = match bid.mtype {
                    MarkupType::BANNER => BidType::Banner,
                    MarkupType::VIDEO => BidType::Video,
                    MarkupType::NATIVE => BidType::Native,
                    other => {
                        errors.push(BidderError::other(format!(
                            "Unable to fetch mediaType in impID: {}, mType: {}",
                            bid.impid, other.0
                        )));
                        continue;
                    }
                };
                bid_response.bids.push(TypedBid::new(bid, t));
            }
        }
        (Some(bid_response), errors)
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

/// Go `template.New("endpointTemplate").Parse(endpoint)`. Go rejects an undefined function
/// (`{{Malformed}}`) at parse time; the shared macros module only fails when resolving, so the
/// template is trial-resolved here to surface that.
fn parse_template(endpoint: &str) -> Result<EndpointTemplate, String> {
    let t = EndpointTemplate::parse(endpoint)?;
    if let Err(e) = t.resolve(&EndpointTemplateParams::default()) {
        if !e.contains("function \".") {
            return Err(e);
        }
    }
    Ok(t)
}
