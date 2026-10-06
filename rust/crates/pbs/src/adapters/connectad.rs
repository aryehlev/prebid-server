//! Go `adapters/connectad/connectad.go`.

use serde::{Deserialize, Deserializer};
use sonic_rs::JsonValueTrait;

use crate::bid_types::BidType;
use crate::bidder::{
    is_response_status_code_no_content, Bidder, BidderResponse, ExtraRequestInfo, RequestData,
    ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{BidRequest, BidResponse, Imp};
use crate::ortb::Ext;

/// Go `jsonutil.StringInt`: an integer sent as a number or as a numeric string.
fn string_int<'de, D: Deserializer<'de>>(d: D) -> Result<i64, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum V {
        I(i64),
        S(String),
        Null(()),
    }
    match V::deserialize(d)? {
        V::I(i) => Ok(i),
        V::Null(()) => Ok(0),
        V::S(s) => {
            if s.is_empty() {
                Ok(0)
            } else {
                s.parse::<i64>().map_err(|_| serde::de::Error::custom(format!("Value is not a number: {s}")))
            }
        }
    }
}

/// Go `openrtb_ext.ExtImpConnectAd`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ExtImpConnectAd {
    #[serde(rename = "networkId", deserialize_with = "string_int")]
    network_id: i64,
    #[serde(rename = "siteId", deserialize_with = "string_int")]
    site_id: i64,
    #[serde(deserialize_with = "crate::ortb::de::float")]
    bidfloor: f64,
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

/// Go: `jsonutil.Unmarshal(imp.Ext, &ExtImpBidder)` then `jsonutil.Unmarshal(bidderExt.Bidder, &T)`.
/// Returns the Go error message text of whichever step fails.
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

fn add_header_if_non_empty(headers: &mut Header, name: &str, value: &str) {
    if !value.is_empty() {
        headers.add(name, value);
    }
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        // Go's `preprocess` rewrites `request.Imp` in place; work on a clone.
        let mut request = request.clone();
        let mut errs = preprocess(&mut request);
        if !errs.is_empty() {
            errs.push(BidderError::bad_input("Error in preprocess of Imp"));
            return (vec![], errs);
        }

        let data = match crate::go_json::to_vec(&request) {
            Ok(d) => d,
            Err(_) => return (vec![], vec![BidderError::bad_input("Error in packaging request to JSON")]),
        };

        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        if let Some(device) = &request.device {
            add_header_if_non_empty(&mut headers, "User-Agent", &device.ua);
            add_header_if_non_empty(&mut headers, "Accept-Language", &device.language);
            if !device.ip.is_empty() {
                add_header_if_non_empty(&mut headers, "X-Forwarded-For", &device.ip);
            } else if !device.ipv6.is_empty() {
                add_header_if_non_empty(&mut headers, "X-Forwarded-For", &device.ipv6);
            }
            match device.dnt {
                Some(dnt) => add_header_if_non_empty(&mut headers, "DNT", &dnt.to_string()),
                None => add_header_if_non_empty(&mut headers, "DNT", "0"),
            }
        }

        (
            vec![RequestData {
                method: "POST".into(),
                uri: self.endpoint.clone(),
                body: data,
                headers,
                imp_ids: request.imp.iter().map(|i| i.id.clone()).collect(),
            }],
            vec![],
        )
    }

    fn make_bids(
        &self,
        _bid_req: &BidRequest,
        _unused: &RequestData,
        http_res: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if is_response_status_code_no_content(http_res) {
            return (None, vec![]);
        }
        if http_res.status_code != 200 {
            return (
                None,
                vec![BidderError::bad_server_response(format!(
                    "Invalid Status Returned: {}. Run with request.debug = 1 for more info",
                    http_res.status_code
                ))],
            );
        }
        let bid_resp: BidResponse = match jsonutil::unmarshal(&http_res.body) {
            Ok(r) => r,
            Err(e) => {
                return (
                    None,
                    vec![BidderError::bad_server_response(format!(
                        "Unable to unpackage bid response. Error: {e}"
                    ))],
                )
            }
        };
        let mut out = BidderResponse::with_bids_capacity(bid_resp.seatbid.len());
        for sb in bid_resp.seatbid {
            for bid in sb.bid {
                out.bids.push(TypedBid::new(bid, BidType::Banner));
            }
        }
        (Some(out), vec![])
    }
}

fn preprocess(request: &mut BidRequest) -> Vec<BidderError> {
    let mut errors = Vec::with_capacity(request.imp.len());
    let mut res_imps = Vec::with_capacity(request.imp.len());
    let mut secure: i8 = 0;

    if let Some(site) = &request.site {
        if !site.page.is_empty() {
            // Go `url.Parse(page)`; only the scheme is looked at, and a parse failure counts as not secure.
            if let Ok(u) = url::Url::parse(&site.page) {
                if u.scheme() == "https" {
                    secure = 1;
                }
            }
        }
    }

    for imp in &request.imp {
        let mut imp = imp.clone();
        let cad_ext = match unpack_imp_ext(&imp) {
            Ok(e) => e,
            Err(e) => {
                errors.push(e);
                continue;
            }
        };
        add_imp_info(&mut imp, secure, &cad_ext);
        if let Err(e) = build_imp_banner(&mut imp) {
            errors.push(e);
            continue;
        }
        res_imps.push(imp);
    }
    request.imp = res_imps;
    errors
}

fn add_imp_info(imp: &mut Imp, secure: i8, cad_ext: &ExtImpConnectAd) {
    imp.tagid = cad_ext.site_id.to_string();
    imp.secure = Some(secure);
    if cad_ext.bidfloor != 0.0 {
        imp.bidfloor = cad_ext.bidfloor;
        imp.bidfloorcur = "USD".to_string();
    }
}

fn unpack_imp_ext(imp: &Imp) -> Result<ExtImpConnectAd, BidderError> {
    // Step 1 failures carry the unmarshal message, step 2 failures a fixed text.
    let ext = match imp.ext.as_ref() {
        None => {
            return Err(BidderError::bad_input(format!(
                "Impression id={} has an Error: expect {{ or n, but found \u{0}",
                imp.id
            )))
        }
        Some(e) => e,
    };
    if !ext.0.is_object() && !ext.0.is_null() {
        return Err(BidderError::bad_input(format!(
            "Impression id={} has an Error: expect {{ or n, but found {}",
            imp.id,
            ext.to_json().chars().next().unwrap_or('\u{0}')
        )));
    }
    let Some(bidder) = ext.0.get("bidder") else {
        return Err(BidderError::bad_input(format!("Impression id={}, has invalid Ext", imp.id)));
    };
    let cad: ExtImpConnectAd = sonic_rs::from_value(bidder)
        .map_err(|_| BidderError::bad_input(format!("Impression id={}, has invalid Ext", imp.id)))?;
    if cad.site_id == 0 {
        return Err(BidderError::bad_input(format!("Impression id={}, has no siteId present", imp.id)));
    }
    Ok(cad)
}

fn build_imp_banner(imp: &mut Imp) -> Result<(), BidderError> {
    imp.ext = None;
    let Some(banner) = imp.banner.as_mut() else {
        return Err(BidderError::bad_input("We need a Banner Object in the request"));
    };
    if banner.w.is_none() && banner.h.is_none() {
        if banner.format.is_empty() {
            return Err(BidderError::bad_input("At least one size is required"));
        }
        let format = banner.format.remove(0);
        banner.w = Some(format.w);
        banner.h = Some(format.h);
    }
    Ok(())
}
