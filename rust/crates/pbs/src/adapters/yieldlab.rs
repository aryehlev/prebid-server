//! Go `adapters/yieldlab/yieldlab.go`.
#![allow(dead_code, unused_imports)]

use crate::bid_types::BidType;
use crate::bidder::{
    check_response_status_code_for_errors, is_response_status_code_no_content, Bidder, BidderResponse,
    ExtraRequestInfo, RequestData, ResponseData, TypedBid,
};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{Bid, BidRequest, BidResponse, Imp};
use crate::ortb::Ext;
use crate::macros::{EndpointTemplate, EndpointTemplateParams};

use std::collections::{BTreeMap, HashMap};

use crate::ortb::openrtb2::SupplyChain;

const AD_SLOT_ID_SEPARATOR: &str = ",";
const ADSIZE_SEPARATOR: &str = "x";

type Generator = Box<dyn Fn() -> String + Send + Sync>;

/// Go `YieldlabAdapter`.
pub struct Adapter {
    endpoint: String,
    cache_buster: Generator,
    get_week: Generator,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            // Go `defaultCacheBuster`: `time.Now().Unix()`.
            cache_buster: Box::new(|| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0)
                    .to_string()
            }),
            // Go `defaultWeekGenerator`: the ISO week number.
            get_week: Box::new(|| iso_week(std::time::SystemTime::now()).to_string()),
        }
    }

    /// The test constructor (Go builds the struct literal with fixed generators).
    pub fn with_generators(
        endpoint: impl Into<String>,
        cache_buster: impl Fn() -> String + Send + Sync + 'static,
        get_week: impl Fn() -> String + Send + Sync + 'static,
    ) -> Self {
        Self { endpoint: endpoint.into(), cache_buster: Box::new(cache_buster), get_week: Box::new(get_week) }
    }
}

/// ISO 8601 week number of a UTC instant.
fn iso_week(t: std::time::SystemTime) -> u32 {
    let secs = t.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let days = secs.div_euclid(86_400);
    // civil year of `days` (Howard Hinnant's algorithm)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + if month <= 2 { 1 } else { 0 };

    fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
        let y = if m <= 2 { y - 1 } else { y };
        let era = y.div_euclid(400);
        let yoe = y.rem_euclid(400);
        let mp = if m > 2 { m - 3 } else { m + 9 };
        let doy = (153 * mp + 2) / 5 + d - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    }
    let jan1 = days_from_civil(year, 1, 1);
    let ordinal = days - jan1 + 1;
    // Monday = 1 .. Sunday = 7 (1970-01-01 was a Thursday)
    let weekday = |d: i64| (d + 3).rem_euclid(7) + 1;
    let is_leap = |y: i64| (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let weeks_in = |y: i64| {
        let wd = weekday(days_from_civil(y, 1, 1));
        if wd == 4 || (is_leap(y) && wd == 3) {
            53
        } else {
            52
        }
    };
    let week = (ordinal - weekday(days) + 10) / 7;
    let week = if week < 1 {
        weeks_in(year - 1)
    } else if week > weeks_in(year) {
        1
    } else {
        week
    };
    week as u32
}

// ── types (Go `types.go`) ────────────────────────────────────────────────────────────────────

#[derive(serde::Deserialize, Default)]
struct ExtImpYieldlab {
    #[serde(rename = "adslotId", default)]
    adslot_id: String,
    #[serde(rename = "supplyId", default)]
    supply_id: String,
    #[serde(default)]
    targeting: Option<HashMap<String, String>>,
    #[serde(rename = "extId", default)]
    ext_id: String,
}

#[derive(serde::Deserialize, Default)]
struct YlBidResponse {
    #[serde(default)]
    id: u64,
    #[serde(default)]
    price: u64,
    #[serde(default)]
    advertiser: String,
    #[serde(default)]
    adsize: String,
    #[serde(default)]
    pid: u64,
    #[serde(default)]
    pvid: String,
    #[serde(default)]
    dsa: Option<DsaResponse>,
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct DsaResponse {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    behalf: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    paid: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    adrender: Option<i64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    transparency: Vec<DsaTransparency>,
}

#[derive(serde::Serialize)]
struct ResponseExtWithDsa<'a> {
    dsa: &'a DsaResponse,
}

#[derive(serde::Serialize, serde::Deserialize, Default, Clone)]
struct DsaTransparency {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    domain: String,
    #[serde(rename = "dsaparams", default, skip_serializing_if = "Vec::is_empty")]
    params: Vec<i64>,
}

/// Go `dsaRequest`.
#[derive(serde::Deserialize, Default)]
struct DsaRequest {
    #[serde(rename = "dsarequired", default)]
    required: Option<i64>,
    #[serde(rename = "pubrender", default)]
    pub_render: Option<i64>,
    #[serde(rename = "datatopub", default)]
    data_to_pub: Option<i64>,
    #[serde(default)]
    transparency: Vec<DsaTransparency>,
}

#[derive(serde::Deserialize, Default)]
struct ExtUser {
    #[serde(default)]
    consent: String,
}

#[derive(serde::Deserialize, Default)]
struct ExtRegsGdpr {
    #[serde(default)]
    gdpr: Option<i8>,
}

#[derive(serde::Deserialize, Default)]
struct ExtSourceSchain {
    #[serde(default)]
    schain: SupplyChain,
}

// ── Go `url.Values` ──────────────────────────────────────────────────────────────────────────

#[derive(Default)]
struct Values(BTreeMap<String, Vec<String>>);

impl Values {
    fn set(&mut self, k: &str, v: impl Into<String>) {
        self.0.insert(k.to_string(), vec![v.into()]);
    }

    /// Go `Values.Encode`: sorted by key.
    fn encode(&self) -> String {
        let mut parts = vec![];
        for (k, vs) in &self.0 {
            for v in vs {
                parts.push(format!("{}={}", query_escape(k), query_escape(v)));
            }
        }
        parts.join("&")
    }
}

/// Go `%v` for a float64 (`%g` with the shortest representation, exponent below -4 / from 21).
fn go_float_v(f: f64) -> String {
    if f == 0.0 {
        return "0".to_string();
    }
    let exp = f.abs().log10().floor() as i32;
    if exp < -4 || exp >= 21 {
        let s = format!("{f:e}");
        // Go writes at least two exponent digits.
        if let Some((m, e)) = s.split_once('e') {
            let (sign, digits) = match e.strip_prefix('-') {
                Some(d) => ('-', d),
                None => ('+', e),
            };
            return format!("{m}e{sign}{digits:0>2}");
        }
        return s;
    }
    format!("{f}")
}

/// Go `path.Join` of a rooted path and one element.
fn path_join(base: &str, elem: &str) -> String {
    let joined = if base.is_empty() { elem.to_string() } else if elem.is_empty() { base.to_string() } else { format!("{base}/{elem}") };
    if joined.is_empty() {
        return String::new();
    }
    let rooted = joined.starts_with('/');
    let mut out: Vec<&str> = vec![];
    for seg in joined.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                if !out.is_empty() && *out.last().unwrap() != ".." {
                    out.pop();
                } else if !rooted {
                    out.push("..");
                }
            }
            s => out.push(s),
        }
    }
    let body = out.join("/");
    if rooted {
        format!("/{body}")
    } else if body.is_empty() {
        ".".to_string()
    } else {
        body
    }
}

// ── request side ─────────────────────────────────────────────────────────────────────────────

impl Adapter {
    /// Go `makeEndpointURL`.
    fn make_endpoint_url(&self, req: &BidRequest, params: &ExtImpYieldlab) -> Result<String, BidderError> {
        let mut uri = url::Url::parse(&self.endpoint).map_err(|_| {
            // Go: `parse "<endpoint>": first path segment in URL cannot contain colon` (the shape
            // the unit test feeds); other parse failures keep the same wrapper.
            BidderError::other(format!(
                "failed to parse yieldlab endpoint: parse \"{}\": first path segment in URL cannot contain colon",
                self.endpoint
            ))
        })?;

        let new_path = path_join(uri.path(), &params.adslot_id);
        let mut q = Values::default();
        for (k, v) in uri.query_pairs() {
            q.0.entry(k.into_owned()).or_default().push(v.into_owned());
        }
        uri.set_path(&new_path);

        q.set("content", "json");
        q.set("pvid", "true");
        q.set("ts", (self.cache_buster)());
        q.set("t", self.make_targeting_values(params));

        let (has_formats, formats) = self.make_formats(req);
        if has_formats {
            q.set("sizes", formats);
        }

        if let Some(user) = &req.user {
            if !user.buyeruid.is_empty() {
                q.set("ids", format!("ylid:{}", user.buyeruid));
            }
        }

        if let Some(device) = &req.device {
            q.set("yl_rtb_ifa", device.ifa.clone());
            q.set("yl_rtb_devicetype", device.devicetype.0.to_string());
            if let Some(ct) = device.connectiontype {
                q.set("yl_rtb_connectiontype", ct.0.to_string());
            }
            if let Some(geo) = &device.geo {
                q.set("lat", go_float_v(geo.lat.unwrap_or_default()));
                q.set("lon", go_float_v(geo.lon.unwrap_or_default()));
            }
        }

        if let Some(app) = &req.app {
            q.set("pubappname", app.name.clone());
            q.set("pubbundlename", app.bundle.clone());
        }

        let (gdpr, consent) = self.get_gdpr(req)?;
        if !gdpr.is_empty() {
            q.set("gdpr", gdpr);
        }
        if !consent.is_empty() {
            q.set("gdpr_consent", consent);
        }

        if let Some(source) = &req.source {
            if source.ext.is_some() {
                if let Some(schain) = unmarshal_supply_chain(req) {
                    let v = make_supply_chain(&schain);
                    if !v.is_empty() {
                        q.set("schain", v);
                    }
                }
            }
        }

        if let Some(dsa) = get_dsa(req)? {
            if let Some(v) = dsa.required {
                q.set("dsarequired", v.to_string());
            }
            if let Some(v) = dsa.pub_render {
                q.set("dsapubrender", v.to_string());
            }
            if let Some(v) = dsa.data_to_pub {
                q.set("dsadatatopub", v.to_string());
            }
            if !dsa.transparency.is_empty() {
                let p = make_dsa_transparency_url_param(&dsa.transparency);
                if !p.is_empty() {
                    q.set("dsatransparency", p);
                }
            }
        }

        uri.set_query(Some(&q.encode()));
        Ok(uri.to_string())
    }

    fn make_formats(&self, req: &BidRequest) -> (bool, String) {
        let mut formats = vec![];
        for imp in &req.imp {
            if !imp_is_type_banner_only(imp) {
                continue;
            }
            let per_adslot: Vec<String> = imp
                .banner
                .as_ref()
                .map(|b| b.format.iter().map(|f| format!("{}x{}", f.w, f.h)).collect())
                .unwrap_or_default();
            let adslot_id = self.extract_adslot_id(imp);
            formats.push(format!("{}:{}", adslot_id, per_adslot.join("|")));
        }
        (!formats.is_empty(), formats.join(","))
    }

    fn get_gdpr(&self, request: &BidRequest) -> Result<(String, String), BidderError> {
        let mut consent = String::new();
        if let Some(ext) = request.user.as_ref().and_then(|u| u.ext.as_ref()) {
            let ext_user: ExtUser = jsonutil::unmarshal(ext.to_json().as_bytes()).map_err(|e| {
                BidderError::other(format!("failed to parse ExtUser in Yieldlab GDPR check: {e}"))
            })?;
            consent = ext_user.consent;
        }
        let mut gdpr = String::new();
        if let Some(regs) = &request.regs {
            if let Ok(r) = unmarshal_ext::<ExtRegsGdpr>(regs.ext.as_ref()) {
                if let Some(g) = r.gdpr {
                    if g == 0 || g == 1 {
                        gdpr = g.to_string();
                    }
                }
            }
        }
        Ok((gdpr, consent))
    }

    fn make_targeting_values(&self, params: &ExtImpYieldlab) -> String {
        let mut values = Values::default();
        if let Some(t) = &params.targeting {
            for (k, v) in t {
                values.set(k, v.clone());
            }
        }
        values.encode()
    }

    /// Go `parseRequest`.
    fn parse_request(&self, request: &BidRequest) -> Vec<ExtImpYieldlab> {
        let mut params = vec![];
        for imp in &request.imp {
            let Ok(bidder_ext) = unmarshal_ext::<ExtImpBidder>(imp.ext.as_ref()) else { continue };
            let Ok(ext) = unmarshal_ext::<ExtImpYieldlab>(bidder_ext.bidder.as_ref()) else { continue };
            params.push(ext);
        }
        params
    }

    /// Go `mergeParams`.
    fn merge_params(&self, params: &[ExtImpYieldlab]) -> ExtImpYieldlab {
        let mut ids = vec![];
        let mut targeting = HashMap::new();
        for p in params {
            ids.push(p.adslot_id.clone());
            if let Some(t) = &p.targeting {
                for (k, v) in t {
                    targeting.insert(k.clone(), v.clone());
                }
            }
        }
        ExtImpYieldlab {
            adslot_id: ids.join(AD_SLOT_ID_SEPARATOR),
            targeting: Some(targeting),
            ..Default::default()
        }
    }

    fn extract_adslot_id(&self, imp: &Imp) -> String {
        let bidder_ext: ExtImpBidder = unmarshal_ext(imp.ext.as_ref()).unwrap_or_default();
        let ext: ExtImpYieldlab = unmarshal_ext(bidder_ext.bidder.as_ref()).unwrap_or_default();
        ext.adslot_id
    }

    fn make_banner_ad_source(&self, req: &BidRequest, ext: &ExtImpYieldlab, res: &YlBidResponse) -> String {
        format!("<script src=\"{}\"></script>", self.make_ad_source_url(req, ext, res))
    }

    fn make_vast(&self, req: &BidRequest, ext: &ExtImpYieldlab, res: &YlBidResponse) -> String {
        format!(
            "<VAST version=\"2.0\"><Ad id=\"{}\"><Wrapper><AdSystem>Yieldlab</AdSystem><VASTAdTagURI><![CDATA[ {} ]]></VASTAdTagURI><Impression></Impression><Creatives></Creatives></Wrapper></Ad></VAST>",
            ext.adslot_id,
            self.make_ad_source_url(req, ext, res)
        )
    }

    fn make_ad_source_url(&self, req: &BidRequest, ext: &ExtImpYieldlab, res: &YlBidResponse) -> String {
        let mut val = Values::default();
        val.set("ts", (self.cache_buster)());
        val.set("id", ext.ext_id.clone());
        val.set("pvid", res.pvid.clone());
        if let Some(user) = &req.user {
            val.set("ids", format!("ylid:{}", user.buyeruid));
        }
        if let Ok((gdpr, consent)) = self.get_gdpr(req) {
            if !gdpr.is_empty() && !consent.is_empty() {
                val.set("gdpr", gdpr);
                val.set("gdpr_consent", consent);
            }
        }
        format!(
            "https://ad.yieldlab.net/d/{}/{}/{}?{}",
            ext.adslot_id,
            ext.supply_id,
            res.adsize,
            val.encode()
        )
    }

    fn make_creative_id(&self, req: &ExtImpYieldlab, bid: &YlBidResponse) -> String {
        format!("{}{}{}", req.adslot_id, bid.pid, (self.get_week)())
    }
}

/// Go `getDSA`: `regs.ext.dsa`, matched case-insensitively like json-iterator.
fn get_dsa(req: &BidRequest) -> Result<Option<DsaRequest>, BidderError> {
    let Some(ext) = req.regs.as_ref().and_then(|r| r.ext.as_ref()) else {
        return Ok(None);
    };
    let wrap = |e: BidderError| {
        BidderError::other(format!("failed to parse Regs.Ext object from Yieldlab response: {e}"))
    };
    let text = ext.to_json();
    let map: serde_json::Map<String, serde_json::Value> =
        jsonutil::unmarshal(text.as_bytes()).map_err(wrap)?;
    // Go: the last matching key wins.
    let Some((_, value)) = map.iter().filter(|(k, _)| k.eq_ignore_ascii_case("dsa")).last() else {
        return Ok(None);
    };
    match value {
        serde_json::Value::Null => Ok(None),
        serde_json::Value::Object(_) => serde_json::from_value::<DsaRequest>(value.clone())
            .map(Some)
            .map_err(|e| wrap(BidderError::FailedToUnmarshal(e.to_string()))),
        other => {
            let first = other.to_string().chars().next().unwrap_or(' ');
            Err(wrap(BidderError::FailedToUnmarshal(format!(
                "cannot unmarshal yieldlab.openRTBExtRegsWithDSA.DSA: expect {{ or n, but found {first}"
            ))))
        }
    }
}

/// Go `makeDSATransparencyURLParam`.
fn make_dsa_transparency_url_param(objs: &[DsaTransparency]) -> String {
    fn one(o: &DsaTransparency, b: &mut String) {
        if o.domain.is_empty() {
            return;
        }
        b.push_str(&o.domain);
        if !o.params.is_empty() {
            b.push('~');
            b.push_str(&o.params.iter().map(|p| p.to_string()).collect::<Vec<_>>().join("_"));
        }
    }
    let mut b = String::new();
    if let Some(first) = objs.first() {
        one(first, &mut b);
        for o in &objs[1..] {
            b.push_str("~~");
            one(o, &mut b);
        }
    }
    b
}

/// Go `unmarshalSupplyChain`: errors are not handled (any ext is accepted).
fn unmarshal_supply_chain(req: &BidRequest) -> Option<SupplyChain> {
    let ext = req.source.as_ref()?.ext.as_ref()?;
    let parsed: ExtSourceSchain = jsonutil::unmarshal(ext.to_json().as_bytes()).ok()?;
    Some(parsed.schain)
}

/// Go `makeSupplyChain`.
fn make_supply_chain(sc: &SupplyChain) -> String {
    if sc.nodes.is_empty() {
        return String::new();
    }
    let mut sb = format!("{},{}", sc.ver, sc.complete);
    for node in &sc.nodes {
        // has to be in order: asi,sid,hp,rid,name,domain,ext
        let hp = node.hp.map(|h| h.to_string()).unwrap_or_default();
        let ext = match &node.ext {
            Some(e) => query_escape(&go_marshal_raw(&e.to_json())),
            None => String::new(),
        };
        sb.push_str(&format!(
            "!{},{},{},{},{},{},{}",
            query_escape(&node.asi),
            query_escape(&node.sid),
            hp,
            query_escape(&node.rid),
            query_escape(&node.name),
            query_escape(&node.domain),
            ext
        ));
    }
    sb
}

/// Go `json.Marshal(json.RawMessage)`: compacted with HTML escaping.
fn go_marshal_raw(compact: &str) -> String {
    compact
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

/// Go `splitSize`.
fn split_size(size: &str) -> Result<(u64, u64), BidderError> {
    let parts: Vec<&str> = size.split(ADSIZE_SEPARATOR).collect();
    if parts.len() != 2 {
        return Ok((0, 0));
    }
    let w = parts[0]
        .parse::<u64>()
        .map_err(|e| BidderError::other(format!("failed to parse yieldlab adsize: {}", parse_uint_err(parts[0], &e))))?;
    let h = parts[1]
        .parse::<u64>()
        .map_err(|e| BidderError::other(format!("failed to parse yieldlab adsize: {}", parse_uint_err(parts[1], &e))))?;
    Ok((w, h))
}

fn parse_uint_err(s: &str, e: &std::num::ParseIntError) -> String {
    use std::num::IntErrorKind::*;
    let reason = match e.kind() {
        PosOverflow => "value out of range",
        _ => "invalid syntax",
    };
    format!("strconv.ParseUint: parsing \"{s}\": {reason}")
}

/// Go `impIsTypeBannerOnly`.
fn imp_is_type_banner_only(imp: &Imp) -> bool {
    imp.banner.is_some() && imp.audio.is_none() && imp.video.is_none() && imp.native.is_none()
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        if request.imp.is_empty() {
            // Go prints the whole request with `%+v`; only the id is reproduced here.
            return (
                vec![],
                vec![BidderError::other(format!(
                    "invalid request {{ID:{}}}, no Impressions given",
                    request.id
                ))],
            );
        }
        let params = self.parse_request(request);
        let merged = self.merge_params(&params);
        let bid_url = match self.make_endpoint_url(request, &merged) {
            Ok(u) => u,
            Err(e) => return (vec![], vec![e]),
        };

        let mut headers = Header::new();
        headers.add("Accept", "application/json");
        if let Some(site) = &request.site {
            headers.add("Referer", site.page.clone());
        }
        if let Some(device) = &request.device {
            headers.add("User-Agent", device.ua.clone());
            headers.add("X-Forwarded-For", device.ip.clone());
        }
        if let Some(user) = &request.user {
            headers.add("Cookie", format!("id={}", user.buyeruid));
        }
        (
            vec![RequestData {
                method: "GET".into(),
                uri: bid_url,
                body: vec![],
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
        if response.status_code != 200 {
            return (
                None,
                vec![BidderError::bad_server_response(format!(
                    "failed to resolve bids from yieldlab response: Unexpected response code {}",
                    response.status_code
                ))],
            );
        }
        let bids: Vec<YlBidResponse> = match jsonutil::unmarshal_any::<Option<Vec<YlBidResponse>>>(&response.body) {
            Ok(b) => b.unwrap_or_default(),
            Err(e) => {
                return (
                    None,
                    vec![BidderError::bad_server_response(format!(
                        "failed to parse bids response from yieldlab: {e}"
                    ))],
                )
            }
        };

        let params = self.parse_request(internal_request);
        let mut bidder_response = BidderResponse::new();
        bidder_response.currency = "EUR".to_string();

        // adslot id -> index of the (last) banner/video imp that uses it
        let mut adslot_to_imp: HashMap<String, usize> = HashMap::new();
        for (i, imp) in internal_request.imp.iter().enumerate() {
            let adslot_id = self.extract_adslot_id(imp);
            if imp.video.is_some() || imp.banner.is_some() {
                adslot_to_imp.insert(adslot_id, i);
            }
        }

        let mut bid_errors = vec![];
        for bid in &bids {
            let (width, height) = match split_size(&bid.adsize) {
                Ok(s) => s,
                Err(e) => return (None, vec![e]),
            };
            let slot = bid.id.to_string();
            let Some(req) = params.iter().find(|p| p.adslot_id == slot) else {
                return (
                    None,
                    vec![BidderError::other(format!(
                        "failed to find yieldlab request for adslotID {}. This is most likely a programming issue",
                        bid.id
                    ))],
                );
            };
            let Some(&imp_idx) = adslot_to_imp.get(&slot) else { continue };
            let imp = &internal_request.imp[imp_idx];

            let ext_json = match &bid.dsa {
                Some(dsa) => match crate::go_json::to_vec(&ResponseExtWithDsa { dsa })
                    .ok()
                    .and_then(|b| Ext::from_slice(&b).ok())
                {
                    Some(e) => Some(e),
                    None => {
                        bid_errors.push(BidderError::other(format!(
                            "failed to make JSON for seatbid.bid.ext for adslotID {}. This is most likely a programming issue",
                            bid.id
                        )));
                        // skip as bids with missing ext.dsa will be discarded anyway
                        continue;
                    }
                },
                None => None,
            };

            let mut response_bid = Bid {
                id: bid.id.to_string(),
                price: bid.price as f64 / 100.0,
                impid: imp.id.clone(),
                crid: self.make_creative_id(req, bid),
                dealid: bid.pid.to_string(),
                w: width as i64,
                h: height as i64,
                adomain: vec![bid.advertiser.clone()],
                ext: ext_json,
                ..Default::default()
            };

            let bid_type;
            if imp.video.is_some() {
                bid_type = BidType::Video;
                response_bid.nurl = self.make_ad_source_url(internal_request, req, bid);
                response_bid.adm = self.make_vast(internal_request, req, bid);
            } else if imp.banner.is_some() {
                bid_type = BidType::Banner;
                response_bid.adm = self.make_banner_ad_source(internal_request, req, bid);
            } else {
                // Yieldlab adapter currently doesn't support Audio and Native ads
                continue;
            }
            bidder_response.bids.push(TypedBid::new(response_bid, bid_type));
        }
        (Some(bidder_response), bid_errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_size_cases() {
        assert_eq!(split_size("300x800").unwrap(), (300, 800));
        assert_eq!(split_size("").unwrap(), (0, 0));
        assert_eq!(split_size("test").unwrap(), (0, 0));
        assert!(split_size("200xtest").is_err());
        assert!(split_size("testx200").is_err());
        assert_eq!(split_size("200y200").unwrap(), (0, 0));
    }

    #[test]
    fn transparency_param() {
        assert_eq!(make_dsa_transparency_url_param(&[]), "");
        let t = |d: &str, p: &[i64]| DsaTransparency { domain: d.into(), params: p.to_vec() };
        assert_eq!(make_dsa_transparency_url_param(&[t("", &[1, 2])]), "");
        assert_eq!(make_dsa_transparency_url_param(&[t("domain.com", &[])]), "domain.com");
        assert_eq!(make_dsa_transparency_url_param(&[t("domain.com", &[1])]), "domain.com~1");
        assert_eq!(
            make_dsa_transparency_url_param(&[t("d1.com", &[1, 2]), t("d2.com", &[3, 4]), t("d3.com", &[5, 6])]),
            "d1.com~1_2~~d2.com~3_4~~d3.com~5_6"
        );
    }

    #[test]
    fn supply_chain() {
        use crate::ortb::openrtb2::SupplyChainNode;
        let mut sc = SupplyChain { ver: "1.0".into(), complete: 1, ..Default::default() };
        assert_eq!(make_supply_chain(&sc), "");
        sc.nodes.push(SupplyChainNode {
            asi: "exchange1.com".into(),
            sid: "12345".into(),
            hp: Some(1),
            ..Default::default()
        });
        assert_eq!(make_supply_chain(&sc), "1.0,1!exchange1.com,12345,1,,,,");
        sc.nodes[0].rid = "bid-request-1".into();
        sc.nodes[0].name = "publisher".into();
        sc.nodes[0].domain = "publisher.com".into();
        sc.nodes[0].ext = Ext::from_slice(br#"{"ext":"test"}"#).ok();
        assert_eq!(
            make_supply_chain(&sc),
            "1.0,1!exchange1.com,12345,1,bid-request-1,publisher,publisher.com,%7B%22ext%22%3A%22test%22%7D"
        );
        sc.nodes[0] = SupplyChainNode { ext: Ext::from_slice(b"1").ok(), ..Default::default() };
        assert_eq!(make_supply_chain(&sc), "1.0,1!,,,,,,1");
    }

    #[test]
    fn node_string_escape() {
        assert_eq!(
            query_escape("AZ09-._~:/?#[]@!$%&'()*+,;="),
            "AZ09-._~%3A%2F%3F%23%5B%5D%40%21%24%25%26%27%28%29%2A%2B%2C%3B%3D"
        );
    }

    #[test]
    fn get_dsa_invalid_request_ext() {
        let req = BidRequest {
            regs: Some(crate::ortb::openrtb2::Regs {
                ext: Ext::from_slice(br#"{"DSA":"wrongValueType"}"#).ok(),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(get_dsa(&req).is_err());
    }

    #[test]
    fn invalid_endpoint() {
        let a = Adapter::new("test$:/something\u{a7}");
        assert!(a.make_endpoint_url(&BidRequest::default(), &ExtImpYieldlab::default()).is_err());
    }

    #[test]
    fn iso_week_known_dates() {
        let at = |secs: u64| std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs);
        // 2021-01-03 is a Sunday in ISO week 53 of 2020.
        assert_eq!(iso_week(at(1_609_632_000)), 53);
        // 2021-01-04 is Monday of week 1.
        assert_eq!(iso_week(at(1_609_718_400)), 1);
        // 2024-12-30 belongs to week 1 of 2025.
        assert_eq!(iso_week(at(1_735_516_800)), 1);
        // 2023-07-15 is in week 28.
        assert_eq!(iso_week(at(1_689_379_200)), 28);
    }

    #[test]
    fn iso_week_matches_date() {
        for p in " 1577836800:1 1609459200:53 1640995200:52 1704067200:1 1735689600:1 1767225600:1 1230768000:1 1262304000:53 1293840000:52 1325376000:52 1356998400:1 1388534400:1 1420070400:1 1451606400:53 1483228800:52 1514764800:1 1546300800:1 1698796800:44 1709251200:9 1709337600:9 1735430400:52".split_whitespace() {
            let (s, w) = p.split_once(':').unwrap();
            let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(s.parse().unwrap());
            assert_eq!(iso_week(t), w.parse::<u32>().unwrap(), "{s}");
        }
    }
}

// ── local helpers (Go `adapters.ExtImpBidder`, `openrtb_ext.ExtBid`) ─────────────────────────

/// Go `adapters.ExtImpBidder` (only `bidder` is used here).
#[derive(serde::Deserialize, Default)]
struct ExtImpBidder {
    #[serde(default)]
    bidder: Option<Ext>,
}

/// Go `jsonutil.Unmarshal(raw, &v)` on a `json.RawMessage` field. An absent message is empty
/// input, which json-iterator rejects with the same `expect { or n, but found` text.
fn unmarshal_ext<T: serde::de::DeserializeOwned>(ext: Option<&Ext>) -> Result<T, BidderError> {
    match ext {
        Some(e) => jsonutil::unmarshal(e.to_json().as_bytes()),
        None => Err(BidderError::FailedToUnmarshal("expect { or n, but found \u{0}".to_string())),
    }
}

/// Go: unmarshal `imp.ext` into `ExtImpBidder`, then `.Bidder` into `T`; the two errors are
/// returned separately so adapters can wrap them differently.
fn parse_imp_ext<T: serde::de::DeserializeOwned>(
    imp: &Imp,
    wrap_bidder_ext: impl Fn(BidderError) -> BidderError,
    wrap_params: impl Fn(BidderError) -> BidderError,
) -> Result<T, BidderError> {
    let bidder_ext: ExtImpBidder = unmarshal_ext(imp.ext.as_ref()).map_err(wrap_bidder_ext)?;
    unmarshal_ext(bidder_ext.bidder.as_ref()).map_err(wrap_params)
}

/// Go `openrtb_ext.ExtBid` (`bid.ext.prebid.type`).
#[derive(serde::Deserialize, Default)]
struct ExtBid {
    #[serde(default)]
    prebid: Option<ExtBidPrebid>,
}

#[derive(serde::Deserialize, Default)]
struct ExtBidPrebid {
    #[serde(rename = "type", default)]
    bid_type: String,
}

/// Go `jsonutil.Unmarshal(bid.Ext, &ExtBid)`: `Err` on a malformed ext.
fn parse_bid_ext(bid: &Bid) -> Option<Result<ExtBid, BidderError>> {
    bid.ext.as_ref().map(|e| jsonutil::unmarshal(e.to_json().as_bytes()))
}

/// Go `strconv.FormatFloat(price, 'f', -1, 64)`.
fn format_price(price: f64) -> String {
    format!("{price}")
}

/// Go `template.New("endpointTemplate").Parse(endpoint)`. `{{Malformed}}` is a Go parse error
/// (unknown function), which `EndpointTemplate::parse` only reports at resolve time, so a dry
/// run with empty params catches it here.
fn parse_template(endpoint: &str) -> Result<EndpointTemplate, String> {
    let t = EndpointTemplate::parse(endpoint)?;
    t.resolve(&EndpointTemplateParams::default())?;
    Ok(t)
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

/// Go `url.PathEscape`.
fn path_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'$' | b'&' | b'+'
            | b'=' | b':' | b'@' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
