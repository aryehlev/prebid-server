//! Go `adapters/adocean/adocean.go`.

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

use std::collections::BTreeMap;

const ADAPTER_VERSION: &str = "1.3.0";
const MAX_URI_LENGTH: usize = 8000;
const MEASUREMENT_CODE: &str = r#"
	<script>
		+function() {
			var wu = "%s";
			var su = "%s".replace(/\[TIMESTAMP\]/, Date.now());

			if (wu && !(navigator.sendBeacon && navigator.sendBeacon(wu))) {
				(new Image(1,1)).src = wu
			}

			if (su && !(navigator.sendBeacon && navigator.sendBeacon(su))) {
				(new Image(1,1)).src = su
			}
		}();
	</script>
"#;

#[derive(Deserialize, Default)]
struct ResponseAdUnit {
    #[serde(default)]
    id: String,
    #[serde(default)]
    crid: String,
    #[serde(default)]
    currency: String,
    #[serde(default)]
    price: String,
    #[serde(default)]
    width: String,
    #[serde(default)]
    height: String,
    #[serde(default)]
    code: String,
    #[serde(rename = "winUrl", alias = "winurl", default)]
    win_url: String,
    #[serde(rename = "statsUrl", default)]
    stats_url: String,
    #[serde(default)]
    error: String,
}

#[derive(Deserialize, Default)]
struct ExtImpAdOcean {
    #[serde(rename = "emitterPrefix", default)]
    emitter_prefix: String,
    #[serde(rename = "masterId", default)]
    master_id: String,
    #[serde(rename = "slaveId", default)]
    slave_id: String,
}

struct PendingRequest {
    url: url::Url,
    headers: Header,
    slave_sizes: BTreeMap<String, String>,
    imp_ids: Vec<String>,
}

pub struct Adapter {
    endpoint_template: EndpointTemplate,
    measurement_code: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl AsRef<str>) -> Result<Self, BidderError> {
        let endpoint_template = parse_template(endpoint.as_ref())
            .map_err(|_| BidderError::other("Unable to parse endpoint template"))?;
        let white_space = regex::Regex::new(r"\s+").map_err(|e| BidderError::other(e.to_string()))?;
        Ok(Self {
            endpoint_template,
            measurement_code: white_space.replace_all(MEASUREMENT_CODE, " ").into_owned(),
        })
    }

    fn add_new_bid(
        &self,
        requests: &mut Vec<PendingRequest>,
        imp: &Imp,
        request: &BidRequest,
        consent: &str,
    ) -> Result<(), BidderError> {
        let bidder = imp_bidder_raw(&imp.ext).map_err(|_| BidderError::bad_input("Error parsing bidderExt object"))?;
        let ext: ExtImpAdOcean =
            unmarshal_raw(&bidder).map_err(|_| BidderError::bad_input("Error parsing adOceanExt parameters"))?;
        if ext.emitter_prefix.is_empty() {
            return Err(BidderError::bad_input("No emitterPrefix param"));
        }
        if add_to_existing_request(requests, &ext, imp, request.test == 1) {
            return Ok(());
        }
        let mut slave_sizes = BTreeMap::new();
        slave_sizes.insert(ext.slave_id.clone(), get_imp_sizes(imp));
        let url = self.make_url(&ext, imp, request, &slave_sizes, consent)?;
        requests.push(PendingRequest {
            url,
            headers: form_headers(request),
            slave_sizes,
            imp_ids: vec![imp.id.clone()],
        });
        Ok(())
    }

    fn make_url(
        &self,
        params: &ExtImpAdOcean,
        imp: &Imp,
        request: &BidRequest,
        slave_sizes: &BTreeMap<String, String>,
        consent: &str,
    ) -> Result<url::Url, BidderError> {
        let host = self
            .endpoint_template
            .resolve(&EndpointTemplateParams { host: params.emitter_prefix.clone(), ..Default::default() })
            .map_err(|e| BidderError::bad_input(format!("Unable to parse endpoint url template: {e}")))?;
        let mut endpoint_url =
            url::Url::parse(&host).map_err(|e| BidderError::bad_input(format!("Malformed URL: {e}")))?;

        let randomized_part = if request.test == 1 {
            10_000_000u64
        } else {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| u64::from(d.subsec_nanos()))
                .unwrap_or(0);
            10_000_000 + nanos % (99_999_999 - 10_000_000)
        };
        endpoint_url.set_path(&format!("/_{randomized_part}/ad.json"));

        let mut q: Query = BTreeMap::new();
        q_add(&mut q, "pbsrv_v", ADAPTER_VERSION);
        q_add(&mut q, "id", &params.master_id);
        q_add(&mut q, "nc", "1");
        q_add(&mut q, "nosecure", "1");
        q_add(&mut q, "aid", &format!("{}:{}", params.slave_id, imp.id));
        if !consent.is_empty() {
            q_add(&mut q, "gdpr_consent", consent);
            q_add(&mut q, "gdpr", "1");
        }
        if let Some(user) = &request.user {
            if !user.buyeruid.is_empty() {
                q_add(&mut q, "hcuserid", &user.buyeruid);
            }
        }
        if let Some(app) = &request.app {
            q_add(&mut q, "app", "1");
            q_add(&mut q, "appname", &app.name);
            q_add(&mut q, "appbundle", &app.bundle);
            q_add(&mut q, "appdomain", &app.domain);
        }
        if let Some(device) = &request.device {
            if !device.ifa.is_empty() {
                q_add(&mut q, "ifa", &device.ifa);
            } else {
                q_add(&mut q, "dpidmd5", &device.dpidmd5);
            }
            q_add(&mut q, "devos", &device.os);
            q_add(&mut q, "devosv", &device.osv);
            q_add(&mut q, "devmodel", &device.model);
            q_add(&mut q, "devmake", &device.make);
        }
        set_slave_sizes_param(&mut q, slave_sizes, request.test == 1);
        endpoint_url.set_query(Some(&q_encode(&q)));
        Ok(endpoint_url)
    }

    fn prepare_ad_code_for_bid(&self, bid: &ResponseAdUnit) -> Result<String, BidderError> {
        let ssp_code = query_unescape_strict(&bid.code).map_err(BidderError::other)?;
        // fmt.Sprintf(measurementCode, winURL, statsURL)
        let measured = self.measurement_code.replacen("%s", &bid.win_url, 1).replacen("%s", &bid.stats_url, 1);
        Ok(measured + &ssp_code)
    }
}

type Query = BTreeMap<String, Vec<String>>;

fn q_add(q: &mut Query, k: &str, v: &str) {
    q.entry(k.to_string()).or_default().push(v.to_string());
}

/// Go `url.Values.Encode`: keys sorted, each value `QueryEscape`d.
fn q_encode(q: &Query) -> String {
    let mut parts = vec![];
    for (k, vs) in q {
        for v in vs {
            parts.push(format!("{}={}", query_escape(k), query_escape(v)));
        }
    }
    parts.join("&")
}

fn q_parse(u: &url::Url) -> Query {
    let mut q: Query = BTreeMap::new();
    for (k, v) in u.query_pairs() {
        q.entry(k.into_owned()).or_default().push(v.into_owned());
    }
    q
}

/// Go `url.QueryUnescape` with its error text.
fn query_unescape_strict(s: &str) -> Result<String, String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' => {
                let hex = b.get(i + 1..i + 3).and_then(|h| std::str::from_utf8(h).ok());
                match hex.and_then(|h| u8::from_str_radix(h, 16).ok().filter(|_| h.bytes().all(|c| c.is_ascii_hexdigit()))) {
                    Some(v) => {
                        out.push(v);
                        i += 3;
                    }
                    None => {
                        let mut end = (i + 3).min(b.len());
                        while !s.is_char_boundary(end) {
                            end -= 1;
                        }
                        return Err(format!("invalid URL escape {:?}", &s[i..end]));
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    Ok(String::from_utf8_lossy(&out).into_owned())
}

fn add_to_existing_request(
    requests: &mut [PendingRequest],
    new_params: &ExtImpAdOcean,
    imp: &Imp,
    test_imp: bool,
) -> bool {
    let auction_id = &imp.id;
    for rd in requests.iter_mut() {
        let mut q = q_parse(&rd.url);
        // Go indexes `queryParams["id"][0]`; every URL built here has an id.
        let master_id = q.get("id").and_then(|v| v.first()).cloned().unwrap_or_default();
        if master_id == new_params.master_id {
            if rd.slave_sizes.contains_key(&new_params.slave_id) {
                continue;
            }
            q_add(&mut q, "aid", &format!("{}:{}", new_params.slave_id, auction_id));
            rd.slave_sizes.insert(new_params.slave_id.clone(), get_imp_sizes(imp));
            set_slave_sizes_param(&mut q, &rd.slave_sizes, test_imp);

            let mut new_url = rd.url.clone();
            new_url.set_query(Some(&q_encode(&q)));
            if new_url.as_str().len() < MAX_URI_LENGTH {
                rd.url = new_url;
                rd.imp_ids.push(auction_id.clone());
                return true;
            }
            rd.slave_sizes.remove(&new_params.slave_id);
        }
    }
    false
}

fn form_headers(req: &BidRequest) -> Header {
    let mut headers = Header::new();
    headers.add("Content-Type", "application/json;charset=utf-8");
    headers.add("Accept", "application/json");
    if let Some(device) = &req.device {
        headers.add("User-Agent", device.ua.clone());
        if !device.ip.is_empty() {
            headers.add("X-Forwarded-For", device.ip.clone());
        } else if !device.ipv6.is_empty() {
            headers.add("X-Forwarded-For", device.ipv6.clone());
        }
    }
    if let Some(site) = &req.site {
        headers.add("Referer", site.page.clone());
    }
    headers
}

fn get_imp_sizes(imp: &Imp) -> String {
    let Some(banner) = &imp.banner else { return String::new() };
    if !banner.format.is_empty() {
        return banner.format.iter().map(|f| format!("{}x{}", f.w, f.h)).collect::<Vec<_>>().join("_");
    }
    if let (Some(w), Some(h)) = (banner.w, banner.h) {
        return format!("{w}x{h}");
    }
    String::new()
}

fn set_slave_sizes_param(q: &mut Query, slave_sizes: &BTreeMap<String, String>, _order_by_key: bool) {
    // Go ranges over a map and sorts only when `orderByKey`; the BTreeMap is always sorted.
    let mut size_values = vec![];
    for (slave_id, sizes) in slave_sizes {
        if sizes.is_empty() {
            continue;
        }
        let raw_slave_id = slave_id.replacen("adocean", "", 1);
        size_values.push(format!("{raw_slave_id}~{sizes}"));
    }
    if !size_values.is_empty() {
        q.insert("aosspsizes".to_string(), vec![size_values.join("-")]);
    }
}

impl Bidder for Adapter {
    fn make_requests(&self, request: &BidRequest, _req_info: &ExtraRequestInfo) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            return (vec![], vec![BidderError::bad_input("No impression in the bid request")]);
        }
        let mut consent = String::new();
        if let Some(user) = &request.user {
            if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&ext_bytes(&user.ext)) {
                if let Some(c) = v.get("consent").and_then(|c| c.as_str()) {
                    consent = c.to_string();
                }
            }
        }

        let mut errs = vec![];
        let mut pending: Vec<PendingRequest> = Vec::with_capacity(request.imp.len());
        for imp in &request.imp {
            if let Err(e) = self.add_new_bid(&mut pending, imp, request, &consent) {
                errs.push(e);
            }
        }
        let http_requests = pending
            .into_iter()
            .map(|r| RequestData {
                method: "GET".into(),
                uri: r.url.to_string(),
                body: vec![],
                headers: r.headers,
                imp_ids: r.imp_ids,
            })
            .collect();
        (http_requests, errs)
    }

    fn make_bids(
        &self,
        _request: &BidRequest,
        external_request: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if response.status_code != 200 {
            return (
                None,
                vec![BidderError::other(format!("Unexpected status code: {}. Network error?", response.status_code))],
            );
        }
        let auction_ids: Vec<String> = url::Url::parse(&external_request.uri)
            .map(|u| q_parse(&u).remove("aid").unwrap_or_default())
            .unwrap_or_default();

        // jsoniter's slice decoder reports its own wording.
        let first = response.body.iter().find(|b| !b" \t\r\n".contains(b)).copied();
        if let Some(c) = first {
            if c != b'[' && c != b'n' {
                return (
                    None,
                    vec![BidderError::FailedToUnmarshal(format!("decode slice: expect [ or n, but found {}", c as char))],
                )
            }
        }
        let bid_responses: Vec<ResponseAdUnit> = match unmarshal_raw::<Option<Vec<ResponseAdUnit>>>(&response.body)
            .or_else(|_| jsonutil::unmarshal_any::<Option<Vec<ResponseAdUnit>>>(&response.body))
        {
            Ok(v) => v.unwrap_or_default(),
            Err(e) => return (None, vec![e]),
        };

        let mut parsed = BidderResponse::with_bids_capacity(auction_ids.len());
        let mut parsing_errors = vec![];
        let mut slave_to_auction: BTreeMap<String, String> = BTreeMap::new();
        for full in &auction_ids {
            // Go indexes `[1]` of SplitN and panics when there is no ":"; skip such entries.
            if let Some((slave, auction)) = full.split_once(':') {
                slave_to_auction.insert(slave.to_string(), auction.to_string());
            }
        }

        for bid in bid_responses {
            let Some(auction_id) = slave_to_auction.get(&bid.id) else { continue };
            if bid.error == "true" {
                continue;
            }
            let price = bid.price.parse::<f64>().unwrap_or(0.0);
            let width = bid.width.parse::<i64>().unwrap_or(0);
            let height = bid.height.parse::<i64>().unwrap_or(0);
            let ad_code = match self.prepare_ad_code_for_bid(&bid) {
                Ok(c) => c,
                Err(e) => {
                    parsing_errors.push(e);
                    continue;
                }
            };
            parsed.bids.push(TypedBid::new(
                Bid {
                    id: bid.id.clone(),
                    impid: auction_id.clone(),
                    price,
                    adm: ad_code,
                    crid: bid.crid.clone(),
                    w: width,
                    h: height,
                    ..Default::default()
                },
                BidType::Banner,
            ));
            parsed.currency = bid.currency.clone();
        }
        (Some(parsed), parsing_errors)
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
