//! Go `adapters/missena/missena.go`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::bid_types::BidType;
use crate::bidder::{Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::macros::{EndpointTemplate, EndpointTemplateParams};
use crate::ortb::openrtb2::{Bid, BidRequest, Imp};
use crate::ortb::Ext;

const DEFAULT_CUR: &str = "USD";

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

fn is_zero_f64(v: &f64) -> bool {
    *v == 0.0
}
fn is_zero_i64(v: &i64) -> bool {
    *v == 0
}

#[derive(Serialize)]
struct MissenaAdRequest<'a> {
    #[serde(skip_serializing_if = "str::is_empty")]
    adunit: &'a str,
    #[serde(skip_serializing_if = "str::is_empty")]
    currency: &'a str,
    #[serde(skip_serializing_if = "is_zero_f64")]
    floor: f64,
    #[serde(skip_serializing_if = "str::is_empty")]
    floor_currency: &'a str,
    #[serde(skip_serializing_if = "str::is_empty")]
    ik: &'a str,
    #[serde(skip_serializing_if = "str::is_empty")]
    request_id: &'a str,
    #[serde(skip_serializing_if = "is_zero_i64")]
    timeout: i64,
    params: UserParams<'a>,
    ortb2: &'a BidRequest,
}

#[derive(Serialize)]
struct UserParams<'a> {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    formats: &'a Vec<String>,
    #[serde(skip_serializing_if = "str::is_empty")]
    placement: &'a str,
    #[serde(rename = "test", skip_serializing_if = "str::is_empty")]
    test_mode: &'a str,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    settings: &'a BTreeMap<String, serde_json::Value>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct BidServerResponse {
    ad: String,
    cpm: f64,
    currency: String,
    #[serde(rename = "requestId")]
    request_id: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpBidder {
    bidder: Option<Ext>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpMissena {
    #[serde(rename = "apiKey")]
    api_key: String,
    formats: Vec<String>,
    placement: String,
    #[serde(rename = "test")]
    test_mode: String,
    settings: BTreeMap<String, serde_json::Value>,
}

fn ext_text(ext: &Option<Ext>) -> Vec<u8> {
    ext.as_ref().map(|e| e.to_json().into_bytes()).unwrap_or_default()
}

fn get_currency(currencies: &[String]) -> Result<&'static str, String> {
    let mut eur_available = false;
    for cur in currencies {
        if cur == DEFAULT_CUR {
            return Ok(DEFAULT_CUR);
        }
        if cur == "EUR" {
            eur_available = true;
        }
    }
    if eur_available {
        return Ok("EUR");
    }
    Err(format!("no currency supported [{}]", currencies.join(" ")))
}

/// Go `url.PathEscape`.
fn path_escape(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b'$' | b'&' | b'+' | b'=' | b':' | b'@' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

impl Adapter {
    fn make_request(
        &self,
        imp: &Imp,
        request: &BidRequest,
        req_info: &ExtraRequestInfo,
        params: &ExtImpMissena,
    ) -> Result<RequestData, BidderError> {
        let endpoint_params = EndpointTemplateParams {
            publisher_id: path_escape(&params.api_key),
            ..Default::default()
        };
        let endpoint_url = self.endpoint_template.resolve(&endpoint_params).map_err(BidderError::other)?;

        let cur = get_currency(&request.cur).unwrap_or(DEFAULT_CUR);
        let mut floor = 0.0;
        let mut floor_cur = "";
        if imp.bidfloor != 0.0 {
            floor = imp.bidfloor;
            match get_currency(&request.cur) {
                Ok(c) => floor_cur = c,
                Err(_) => {
                    floor_cur = DEFAULT_CUR;
                    floor = req_info.convert_currency(imp.bidfloor, &imp.bidfloorcur, floor_cur)?;
                }
            }
        }
        let missena_request = MissenaAdRequest {
            adunit: &imp.id,
            currency: cur,
            floor,
            floor_currency: floor_cur,
            ik: &request.id,
            request_id: &request.id,
            timeout: request.tmax,
            params: UserParams {
                formats: &params.formats,
                placement: &params.placement,
                test_mode: &params.test_mode,
                settings: &params.settings,
            },
            ortb2: request,
        };
        let body = crate::go_json::to_vec(&missena_request)
            .map_err(|e| BidderError::FailedToMarshal(e.to_string()))?;

        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        if let Some(device) = &request.device {
            headers.add("User-Agent", device.ua.clone());
            if !device.ip.is_empty() {
                headers.add("X-Forwarded-For", device.ip.clone());
            } else if !device.ipv6.is_empty() {
                headers.add("X-Forwarded-For", device.ipv6.clone());
            }
        }
        if let Some(site) = &request.site {
            if !site.page.is_empty() {
                headers.add("Referer", site.page.clone());
                if let Some(origin) = origin_of(&site.page) {
                    headers.add("Origin", origin);
                }
            }
        }
        Ok(RequestData {
            method: "POST".into(),
            uri: endpoint_url,
            body,
            headers,
            imp_ids: vec![imp.id.clone()],
        })
    }
}

/// `scheme://host` from Go's `url.Parse`; `None` when the parse fails.
fn origin_of(page: &str) -> Option<String> {
    match url::Url::parse(page) {
        Ok(u) => {
            let mut host = u.host_str().unwrap_or_default().to_string();
            if let Some(p) = u.port() {
                host = format!("{host}:{p}");
            }
            Some(format!("{}://{}", u.scheme(), host))
        }
        // Go parses a relative reference without error: empty scheme and host.
        Err(url::ParseError::RelativeUrlWithoutBase) => Some("://".to_string()),
        Err(_) => None,
    }
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut http_requests = vec![];
        let mut errors = vec![];
        for imp in &request.imp {
            let bidder_ext: ExtImpBidder = match jsonutil::unmarshal(&ext_text(&imp.ext)) {
                Ok(e) => e,
                Err(e) => {
                    errors.push(BidderError::bad_input(format!(
                        "Error parsing bidderExt object: {}, input: {}",
                        e,
                        String::from_utf8_lossy(&ext_text(&imp.ext))
                    )));
                    continue;
                }
            };
            // Go unmarshals into `*ExtImpMissena`: `null` leaves a nil pointer that
            // `makeRequest` dereferences (panic); an error is returned here instead.
            let bytes = ext_text(&bidder_ext.bidder);
            let params: ExtImpMissena = match jsonutil::unmarshal(&bytes) {
                Ok(p) if bytes != b"null" => p,
                _ => {
                    errors.push(BidderError::bad_input("Error parsing missenaExt parameters"));
                    continue;
                }
            };
            match self.make_request(imp, request, req_info, &params) {
                Ok(r) => {
                    http_requests.push(r);
                    // Only one impression per request: stop at the first working one.
                    break;
                }
                Err(e) => errors.push(e),
            }
        }
        (http_requests, errors)
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
        if response.status_code == 400 {
            return (
                None,
                vec![BidderError::bad_input(
                    "Unexpected status code: 400. Bad request from publisher. Run with request.debug = 1 for more info.",
                )],
            );
        }
        if response.status_code != 200 {
            return (
                None,
                vec![BidderError::bad_server_response(format!(
                    "Unexpected status code: {}. Run with request.debug = 1 for more info.",
                    response.status_code
                ))],
            );
        }
        let missena: BidServerResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        // Go indexes `request.Imp[0]` and panics on an empty imp list; report an error instead.
        let Some(first) = request.imp.first() else {
            return (None, vec![BidderError::bad_server_response("no impressions in the request")]);
        };
        let mut bid_res = BidderResponse::with_bids_capacity(1);
        bid_res.currency = missena.currency;
        let bid = Bid {
            id: request.id.clone(),
            price: missena.cpm,
            impid: first.id.clone(),
            adm: missena.ad,
            crid: missena.request_id,
            ..Default::default()
        };
        bid_res.bids.push(TypedBid::new(bid, BidType::Banner));
        (Some(bid_res), vec![])
    }
}
