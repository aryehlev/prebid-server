//! Go `adapters/tappx/tappx.go`.

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use regex::Regex;
use serde::{Deserialize, Serialize};
use sonic_rs::JsonValueTrait;

use crate::bid_types::BidType;
use crate::bidder::{
    is_response_status_code_no_content, Bidder, BidderResponse, ExtraRequestInfo, RequestData,
    ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::macros::{EndpointTemplate, EndpointTemplateParams};
use crate::ortb::openrtb2::{BidRequest, BidResponse, Imp};
use crate::ortb::Ext;

const TAPPX_BIDDER_VERSION: &str = "1.6";
const TYPE_CNN: &str = "prebid";

/// Go `openrtb_ext.ExtImpTappx`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ExtImpTappx {
    tappxkey: String,
    endpoint: String,
    #[serde(deserialize_with = "crate::ortb::de::float")]
    bidfloor: f64,
    mktag: String,
    bcid: Vec<String>,
    bcrid: Vec<String>,
}

/// Go `Bidder` (the `ext.bidder` object written to `request.ext`).
#[derive(Serialize)]
struct ExtBidder<'a> {
    tappxkey: &'a str,
    #[serde(skip_serializing_if = "str::is_empty")]
    mktag: &'a str,
    #[serde(skip_serializing_if = "<[String]>::is_empty")]
    bcid: &'a [String],
    #[serde(skip_serializing_if = "<[String]>::is_empty")]
    bcrid: &'a [String],
}

#[derive(Serialize)]
struct ReqExt<'a> {
    bidder: ExtBidder<'a>,
}

pub struct Adapter {
    endpoint_template: EndpointTemplate,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: &str) -> Result<Self, BidderError> {
        let t = EndpointTemplate::parse(endpoint)
            .map_err(|e| BidderError::other(format!("unable to parse endpoint url template: {e}")))?;
        Ok(Self { endpoint_template: t })
    }

    /// Go `buildEndpointURL`.
    fn build_endpoint_url(&self, params: &ExtImpTappx, test: i64) -> Result<String, BidderError> {
        if params.endpoint.is_empty() {
            return Err(BidderError::bad_input("Tappx endpoint undefined"));
        }
        if params.tappxkey.is_empty() {
            return Err(BidderError::bad_input("Tappx key undefined"));
        }
        let re = Regex::new(r"^(zz|vz)[0-9]{3,}([a-z]{2,3}|test)$").expect("static regex");
        let is_new = re.is_match(&params.endpoint);
        let tappx_host = if is_new {
            format!("{}.pub.tappx.com/rtb/", params.endpoint)
        } else {
            "ssp.api.tappx.com/rtb/v2/".to_string()
        };
        let p = EndpointTemplateParams { host: tappx_host, ..Default::default() };
        let host = self.endpoint_template.resolve(&p).map_err(|e| {
            BidderError::bad_input(format!("Unable to parse endpoint url template: {e}"))
        })?;
        let mut uri = url::Url::parse(&host)
            .map_err(|e| BidderError::bad_input(format!("Malformed URL: {e}")))?;
        if !is_new {
            let path = format!("{}{}", uri.path(), params.endpoint);
            uri.set_path(&path);
        }

        // Go `url.Values.Encode` sorts by key.
        let mut q: BTreeMap<&str, String> = BTreeMap::new();
        q.insert("tappxkey", params.tappxkey.clone());
        if test == 0 {
            let t = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
            q.insert("ts", t.to_string());
        }
        q.insert("v", TAPPX_BIDDER_VERSION.to_string());
        q.insert("type_cnn", TYPE_CNN.to_string());
        let query = q
            .iter()
            .map(|(k, v)| format!("{}={}", query_escape(k), query_escape(v)))
            .collect::<Vec<_>>()
            .join("&");
        uri.set_query(Some(&query));
        Ok(uri.to_string())
    }
}

/// Go `url.QueryEscape`.
fn query_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Go: `Unmarshal(imp.Ext, &ExtImpBidder)` then `Unmarshal(bidderExt.Bidder, &T)`.
fn imp_bidder_params<T: serde::de::DeserializeOwned>(ext: Option<&Ext>) -> Result<T, String> {
    fn not_obj(v: &sonic_rs::Value) -> Option<String> {
        if v.is_object() || v.is_null() {
            return None;
        }
        let first = sonic_rs::to_string(v).ok()?.chars().next().unwrap_or('\u{0}');
        Some(format!("expect {{ or n, but found {first}"))
    }
    let Some(ext) = ext else {
        return Err("expect { or n, but found \u{0}".to_string());
    };
    if let Some(m) = not_obj(&ext.0) {
        return Err(m);
    }
    match ext.0.get("bidder") {
        None => Err("expect { or n, but found \u{0}".to_string()),
        Some(b) => {
            if let Some(m) = not_obj(b) {
                return Err(m);
            }
            sonic_rs::from_value::<T>(b).map_err(|e| e.to_string())
        }
    }
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::bad_input("No impression in the bid request")]);
        }
        let tappx_ext: ExtImpTappx = match imp_bidder_params_two(request.imp[0].ext.as_ref()) {
            Ok(e) => e,
            Err(m) => return (vec![], vec![BidderError::bad_input(m)]),
        };

        // Go mutates the request in place (`request.Ext`, `request.Imp[0].BidFloor`); use a clone.
        let mut request = request.clone();
        let ext = ReqExt {
            bidder: ExtBidder {
                tappxkey: &tappx_ext.tappxkey,
                mktag: &tappx_ext.mktag,
                bcid: &tappx_ext.bcid,
                bcrid: &tappx_ext.bcrid,
            },
        };
        let jsonext = match crate::go_json::to_vec(&ext)
            .map_err(|e| e.to_string())
            .and_then(|b| Ext::from_slice(&b).map_err(|e| e.to_string()))
        {
            Ok(e) => e,
            Err(_) => {
                return (vec![], vec![BidderError::failed_to_request_bids("Error marshaling tappxExt parameters")])
            }
        };
        request.ext = Some(jsonext);

        let test = request.test as i64;
        let url = match self.build_endpoint_url(&tappx_ext, test) {
            Ok(u) => u,
            Err(e) => return (vec![], vec![e]),
        };

        if tappx_ext.bidfloor > 0.0 {
            request.imp[0].bidfloor = tappx_ext.bidfloor;
        }

        let body = match crate::go_json::to_vec(&request) {
            Ok(b) => b,
            Err(_) => return (vec![], vec![BidderError::bad_input("Error parsing reqJSON object")]),
        };

        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        (
            vec![RequestData {
                method: "POST".into(),
                uri: url,
                body,
                headers,
                imp_ids: request.imp.iter().map(|i| i.id.clone()).collect(),
            }],
            vec![],
        )
    }

    fn make_bids(
        &self,
        internal_request: &BidRequest,
        _external_request: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if is_response_status_code_no_content(response) {
            return (None, vec![]);
        }
        let msg = format!(
            "Unexpected status code: {}. Run with request.debug = 1 for more info",
            response.status_code
        );
        if response.status_code == 400 {
            return (None, vec![BidderError::bad_input(msg)]);
        }
        if response.status_code != 200 {
            return (None, vec![BidderError::other(msg)]);
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&response.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        let mut out = BidderResponse::with_bids_capacity(5);
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                let t = get_media_type_for_imp(&bid.impid, &internal_request.imp);
                out.bids.push(TypedBid::new(bid, t));
            }
        }
        (Some(out), vec![])
    }
}

/// Go reports two fixed texts: step 1 failure is `Error parsing bidderExt object`, step 2 failure
/// `Error parsing tappxExt parameters`.
fn imp_bidder_params_two<T: serde::de::DeserializeOwned>(ext: Option<&Ext>) -> Result<T, &'static str> {
    let outer_ok = ext.is_some_and(|e| e.0.is_object() || e.0.is_null());
    if !outer_ok {
        return Err("Error parsing bidderExt object");
    }
    imp_bidder_params(ext).map_err(|_| "Error parsing tappxExt parameters")
}

fn get_media_type_for_imp(imp_id: &str, imps: &[Imp]) -> BidType {
    for imp in imps {
        if imp.id == imp_id {
            if imp.video.is_some() {
                return BidType::Video;
            }
            return BidType::Banner;
        }
    }
    BidType::Banner
}
