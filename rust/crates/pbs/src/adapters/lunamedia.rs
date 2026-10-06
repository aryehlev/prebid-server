//! Go `adapters/lunamedia/lunamedia.go`.

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

#[derive(Deserialize, Default, Clone, PartialEq, Eq, Hash)]
struct ExtImpLunaMedia {
    #[serde(rename = "pubid", default)]
    publisher_id: String,
    #[serde(default)]
    placement: String,
}

pub struct Adapter {
    endpoint_template: EndpointTemplate,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl AsRef<str>) -> Result<Self, BidderError> {
        let endpoint_template = parse_template(endpoint.as_ref())
            .map_err(|e| BidderError::other(format!("unable to parse endpoint url template: {e}")))?;
        Ok(Self { endpoint_template })
    }
}

fn get_impression_ext(imp: &Imp) -> Result<ExtImpLunaMedia, BidderError> {
    let bidder = imp_bidder_raw(&imp.ext).map_err(|e| BidderError::bad_input(e.to_string()))?;
    unmarshal_raw(&bidder).map_err(|e| BidderError::bad_input(e.to_string()))
}

/// Go `compatImpression`.
fn compat_impression(imp: &mut Imp) -> Result<(), BidderError> {
    imp.ext = None; // do not forward ext to LunaMedia platform
    if let Some(banner) = imp.banner.as_mut() {
        // As banner.w/h are required fields for LunaMedia platform - take the first format entry
        if banner.w.is_none() || banner.h.is_none() {
            if banner.format.is_empty() {
                return Err(BidderError::bad_input("Expected at least one banner.format entry or explicit w/h"));
            }
            let format = banner.format.remove(0);
            banner.w = Some(format.w);
            banner.h = Some(format.h);
        }
    }
    Ok(())
}

impl Bidder for Adapter {
    fn make_requests(&self, request: &BidRequest, _req_info: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = vec![];
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::bad_input("No impression in the bid request")]);
        }
        // Group impressions by LunaMedia-specific parameters. Go ranges over a map, so the order of
        // the resulting requests is unspecified there; first-seen order is used here.
        let mut groups: Vec<(ExtImpLunaMedia, Vec<Imp>)> = vec![];
        let mut valid = 0;
        for imp in &request.imp {
            let ext = match get_impression_ext(imp) {
                Ok(e) => e,
                Err(e) => {
                    errs.push(e);
                    continue;
                }
            };
            if ext.publisher_id.is_empty() {
                errs.push(BidderError::bad_input("No pubid value provided"));
                continue;
            }
            let mut imp = imp.clone();
            if let Err(e) = compat_impression(&mut imp) {
                errs.push(e);
                continue;
            }
            valid += 1;
            match groups.iter_mut().find(|(k, _)| *k == ext) {
                Some((_, v)) => v.push(imp),
                None => groups.push((ext, vec![imp])),
            }
        }
        if valid == 0 {
            return (vec![], errs);
        }

        let mut result = Vec::with_capacity(groups.len());
        for (params, imps) in groups {
            let mut req = request.clone();
            req.imp = imps;
            for imp in &mut req.imp {
                imp.tagid = params.placement.clone();
            }
            if let Some(site) = req.site.as_mut() {
                site.publisher = None;
                site.domain = String::new();
            }
            if let Some(app) = req.app.as_mut() {
                app.publisher = None;
            }
            let body = match marshal(&req) {
                Ok(b) => b,
                Err(e) => {
                    errs.push(e);
                    return (vec![], errs);
                }
            };
            let mut headers = Header::new();
            headers.add("Content-Type", "application/json;charset=utf-8");
            headers.add("Accept", "application/json");
            headers.add("x-openrtb-version", "2.5");
            let url = match self
                .endpoint_template
                .resolve(&EndpointTemplateParams { publisher_id: params.publisher_id.clone(), ..Default::default() })
            {
                Ok(u) => u,
                Err(e) => {
                    errs.push(BidderError::other(e));
                    return (vec![], errs);
                }
            };
            result.push(RequestData { method: "POST".into(), uri: url, body, headers, imp_ids: imp_ids(&req.imp) });
        }
        (result, errs)
    }

    fn make_bids(
        &self,
        request: &BidRequest,
        _request_data: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if response.status_code == 204 {
            return (None, vec![]);
        }
        if response.status_code != 200 {
            return (
                None,
                vec![BidderError::bad_server_response(format!("Unexpected http status code: {}", response.status_code))],
            );
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            // Go formats the error with `%d`.
            Err(e) => return (None, vec![BidderError::bad_server_response(format!("Bad server response: {}", fmt_d(&e)))]),
        };
        if bid_resp.seatbid.len() != 1 {
            return (
                None,
                vec![BidderError::bad_server_response(format!("Invalid SeatBids count: {}", bid_resp.seatbid.len()))],
            );
        }
        let seat_bid = bid_resp.seatbid.into_iter().next().unwrap_or_default();
        let mut bid_response = BidderResponse::with_bids_capacity(seat_bid.bid.len());
        for bid in seat_bid.bid {
            let t = media_type_for_imp_id(&bid.impid, &request.imp);
            bid_response.bids.push(TypedBid::new(bid, t));
        }
        (Some(bid_response), vec![])
    }
}

/// Go `fmt.Sprintf("%d", err)` on a `*errortypes.FailedToUnmarshal` (one string field).
fn fmt_d(e: &BidderError) -> String {
    format!("&{{%!d(string={})}}", e.message())
}

fn media_type_for_imp_id(imp_id: &str, imps: &[Imp]) -> BidType {
    for imp in imps {
        if imp.id == imp_id && imp.video.is_some() {
            return BidType::Video;
        }
    }
    BidType::Banner
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
