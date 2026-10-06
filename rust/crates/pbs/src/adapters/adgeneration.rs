//! Go `adapters/adgeneration/adgeneration.go`.

use regex::Regex;
use serde::Deserialize;

use crate::bid_types::BidType;
use crate::bidder::{Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{Bid, BidRequest, Imp};
use crate::ortb::Ext;

pub struct Adapter {
    endpoint: String,
    version: String,
    default_currency: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpAdgeneration {
    id: String,
}

/// Go `adgServerResponse` (keys lower-cased: json-iterator matches case-insensitively).
#[derive(Deserialize, Default)]
#[serde(default)]
struct AdgServerResponse {
    locationid: String,
    dealid: String,
    ad: String,
    beacon: String,
    cpm: f64,
    creativeid: String,
    h: u64,
    w: u64,
    vastxml: String,
    results: Vec<serde_json::Value>,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Result<Self, BidderError> {
        Ok(Self { endpoint: endpoint.into(), version: "1.0.3".into(), default_currency: "JPY".into() })
    }

    fn get_request_uri(&self, request: &BidRequest, index: usize) -> Result<String, BidderError> {
        let imp = &request.imp[index];
        let ext = unmarshal_ext_imp_adgeneration(imp).map_err(|e| BidderError::bad_input(e.to_string()))?;
        let mut uri = url::Url::parse(&self.endpoint).map_err(|e| BidderError::bad_input(e.to_string()))?;
        let q = self.get_raw_query(&ext.id, request, imp);
        uri.set_query(Some(&q));
        Ok(uri.to_string())
    }

    fn get_raw_query(&self, id: &str, request: &BidRequest, imp: &Imp) -> String {
        let mut v: std::collections::BTreeMap<&str, String> = std::collections::BTreeMap::new();
        v.insert("posall", "SSPLOC".into());
        v.insert("id", id.into());
        v.insert("hb", "true".into());
        v.insert("t", "json3".into());
        v.insert("currency", self.get_currency(request));
        v.insert("sdkname", "prebidserver".into());
        v.insert("adapterver", self.version.clone());
        let ad_size = get_sizes(imp);
        if !ad_size.is_empty() {
            v.insert("sizes", ad_size);
        }
        let os = request.device.as_ref().map(|d| d.os.as_str());
        if os == Some("android") {
            v.insert("sdktype", "1".into());
        } else if os == Some("ios") {
            v.insert("sdktype", "2".into());
        } else {
            v.insert("sdktype", "0".into());
        }
        if let Some(site) = &request.site {
            if !site.page.is_empty() {
                v.insert("tp", site.page.clone());
            }
        }
        if let Some(src) = &request.source {
            if !src.tid.is_empty() {
                v.insert("transactionid", src.tid.clone());
            }
        }
        if let Some(app) = &request.app {
            if !app.bundle.is_empty() {
                v.insert("appbundle", app.bundle.clone());
            }
            if !app.name.is_empty() {
                v.insert("appname", app.name.clone());
            }
        }
        if let Some(d) = &request.device {
            if d.os == "ios" && !d.ifa.is_empty() {
                v.insert("idfa", d.ifa.clone());
            }
            if d.os == "android" && !d.ifa.is_empty() {
                v.insert("advertising_id", d.ifa.clone());
            }
        }
        v.iter().map(|(k, val)| format!("{}={}", query_escape(k), query_escape(val))).collect::<Vec<_>>().join("&")
    }

    fn get_currency(&self, request: &BidRequest) -> String {
        if request.cur.is_empty() {
            return self.default_currency.clone();
        }
        for c in &request.cur {
            if &self.default_currency == c {
                return c.clone();
            }
        }
        request.cur[0].clone()
    }
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

fn unmarshal_ext_imp_adgeneration(imp: &Imp) -> Result<ExtImpAdgeneration, BidderError> {
    let bidder_ext: ExtImpBidder = unmarshal_ext(imp.ext.as_ref())?;
    let adg: ExtImpAdgeneration = unmarshal_ext(bidder_ext.bidder.as_ref())?;
    if adg.id.is_empty() {
        return Err(BidderError::other("No Location ID in ExtImpAdgeneration."));
    }
    Ok(adg)
}

fn get_sizes(imp: &Imp) -> String {
    let Some(banner) = &imp.banner else { return String::new() };
    if banner.format.is_empty() {
        return String::new();
    }
    banner.format.iter().map(|f| format!("{}x{}", f.w, f.h)).collect::<Vec<_>>().join(",")
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let n = request.imp.len();
        if n == 0 {
            return (vec![], vec![BidderError::bad_input("No impression in the bid request")]);
        }
        let mut headers = Header::new();
        headers.add("Content-Type", "application/json;charset=utf-8");
        headers.add("Accept", "application/json");
        if let Some(d) = &request.device {
            if !d.ua.is_empty() {
                headers.add("User-Agent", d.ua.clone());
            }
            if !d.ip.is_empty() {
                headers.add("X-Forwarded-For", d.ip.clone());
            }
        }
        let mut out = Vec::with_capacity(n);
        for index in 0..n {
            let uri = match self.get_request_uri(request, index) {
                Ok(u) => u,
                Err(e) => return (vec![], vec![e]),
            };
            out.push(RequestData {
                method: "GET".into(),
                uri,
                body: Vec::new(),
                headers: headers.clone(),
                imp_ids: vec![request.imp[index].id.clone()],
            });
        }
        (out, vec![])
    }

    fn make_bids(
        &self,
        internal_request: &BidRequest,
        _external_request: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if response.status_code == 204 {
            return (None, vec![]);
        }
        let msg = || format!("Unexpected status code: {}. Run with request.debug = 1 for more info", response.status_code);
        if response.status_code == 400 {
            return (None, vec![BidderError::bad_input(msg())]);
        }
        if response.status_code != 200 {
            return (None, vec![BidderError::bad_server_response(msg())]);
        }
        let bid_resp: AdgServerResponse = match unmarshal_bytes(&response.body) {
            Ok(r) => r,
            Err(e) => return (None, vec![e]),
        };
        if bid_resp.results.is_empty() {
            return (None, vec![]);
        }
        let mut bid_response = BidderResponse::with_bids_capacity(1);
        for v in &internal_request.imp {
            let adg = match unmarshal_ext_imp_adgeneration(v) {
                Ok(a) => a,
                Err(e) => return (None, vec![BidderError::bad_server_response(e.to_string())]),
            };
            if adg.id == bid_resp.locationid {
                let imp_id = v.id.clone();
                let adm = create_ad(&bid_resp, &imp_id);
                let bid = Bid {
                    id: bid_resp.locationid.clone(),
                    impid: imp_id,
                    adm,
                    price: bid_resp.cpm,
                    w: bid_resp.w as i64,
                    h: bid_resp.h as i64,
                    crid: bid_resp.creativeid.clone(),
                    dealid: bid_resp.dealid.clone(),
                    ..Default::default()
                };
                bid_response.bids.push(TypedBid::new(bid, BidType::Banner));
                bid_response.currency = self.get_currency(internal_request);
                return (Some(bid_response), vec![]);
            }
        }
        (None, vec![])
    }
}

/// Go `jsonutil.Unmarshal` into a struct whose keys are all lower case.
fn unmarshal_bytes<T: serde::de::DeserializeOwned + Default>(body: &[u8]) -> Result<T, BidderError> {
    let first = body.iter().find(|b| !b" \t\r\n".contains(b)).copied();
    match first {
        Some(b'{') => {}
        Some(b'n') => return Ok(T::default()),
        Some(c) => {
            return Err(BidderError::FailedToUnmarshal(format!("expect {{ or n, but found {}", c as char)))
        }
        None => return jsonutil::unmarshal(body),
    }
    let map: serde_json::Map<String, serde_json::Value> =
        jsonutil::unmarshal_any(body)?;
    let lowered: serde_json::Map<String, serde_json::Value> =
        map.into_iter().map(|(k, v)| (k.to_lowercase(), v)).collect();
    serde_json::from_value(serde_json::Value::Object(lowered))
        .map_err(|e| BidderError::FailedToUnmarshal(e.to_string()))
}

fn create_ad(body: &AdgServerResponse, imp_id: &str) -> String {
    let mut ad = body.ad.clone();
    if !body.vastxml.is_empty() {
        ad = format!(
            "<body><div id=\"apvad-{imp_id}\"></div><script type=\"text/javascript\" id=\"apv\" src=\"https://cdn.apvdr.com/js/VideoAd.min.js\"></script>{}</body>",
            insert_vast_method(imp_id, &body.vastxml)
        );
    }
    ad = append_child_to_body(&ad, &body.beacon);
    let unwrapped = remove_wrapper(&ad);
    if !unwrapped.is_empty() {
        return unwrapped;
    }
    ad
}

fn insert_vast_method(bid_id: &str, vastxml: &str) -> String {
    // Go compiles the literal pattern `/\r?\n/g` (slashes included), which rarely matches.
    let rep = Regex::new(r"/\r?\n/g").expect("regex");
    let replaced = rep.replace_all(vastxml, "");
    format!(
        "<script type=\"text/javascript\"> (function(){{ new APV.VideoAd({{s:\"{bid_id}\"}}).load('{replaced}'); }})(); </script>"
    )
}

fn append_child_to_body(ad: &str, data: &str) -> String {
    let rep = Regex::new(r"</\s?body>").expect("regex");
    rep.replace_all(ad, format!("{data}</body>").as_str()).into_owned()
}

fn remove_wrapper(ad: &str) -> String {
    let (Some(body_index), Some(last_body_index)) = (ad.find("<body>"), ad.rfind("</body>")) else {
        return String::new();
    };
    // Go slices `ad[bodyIndex:lastBodyIndex]` (panics if reversed); return empty instead.
    if body_index > last_body_index {
        return String::new();
    }
    ad[body_index..last_body_index].replacen("<body>", "", 1).replacen("</body>", "", 1).trim().to_string()
}

// ---- local helpers (shared foundation untouched) ----

/// Go `jsonutil.Unmarshal(ext, &T)`: json-iterator matches keys case-insensitively; a non-object
/// (other than null) reports `expect { or n, but found X`; absent ext is empty input.
fn unmarshal_ext<T: serde::de::DeserializeOwned + Default>(ext: Option<&Ext>) -> Result<T, BidderError> {
    use sonic_rs::JsonValueTrait;
    let Some(ext) = ext else {
        return Err(BidderError::FailedToUnmarshal("expect { or n, but found \u{0}".into()));
    };
    if ext.0.is_null() {
        return Ok(T::default());
    }
    let text = ext.to_json();
    if !ext.0.is_object() {
        let c = text.chars().next().unwrap_or('\u{0}');
        return Err(BidderError::FailedToUnmarshal(format!("expect {{ or n, but found {c}")));
    }
    let map: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&text)
        .map_err(|e| BidderError::FailedToUnmarshal(e.to_string()))?;
    let lowered: serde_json::Map<String, serde_json::Value> =
        map.into_iter().map(|(k, v)| (k.to_lowercase(), v)).collect();
    serde_json::from_value(serde_json::Value::Object(lowered))
        .map_err(|e| BidderError::FailedToUnmarshal(e.to_string()))
}

#[derive(serde::Deserialize, Default)]
#[serde(default)]
struct ExtImpBidder {
    bidder: Option<Ext>,
}
