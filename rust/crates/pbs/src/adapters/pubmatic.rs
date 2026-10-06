//! Go `adapters/pubmatic/pubmatic.go`.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use sonic_rs::JsonValueTrait;

use crate::bid_types::{BidType, ExtBidPrebidMeta, ExtBidPrebidVideo};
use crate::bidder::{Bidder, BidderResponse, ExtraRequestInfo, RequestData, ResponseData, TypedBid};
use crate::errortypes::BidderError;
use crate::header::Header;
use crate::jsonutil;
use crate::ortb::openrtb2::{App, Banner, Bid, BidRequest, BidResponse, Imp, MarkupType, Publisher};
use crate::ortb::Ext;

const AE: &str = "ae";
const BIDDER_PUBMATIC: &str = "pubmatic";
const DCTR_KEY_NAME: &str = "key_val";
const PM_ZONE_ID_KEY_NAME: &str = "pmZoneId";
const PM_ZONE_ID_KEY_NAME_OLD: &str = "pmZoneID";
const IMP_EXT_AD_UNIT_KEY: &str = "dfp_ad_unit_code";
const AD_SERVER_GAM: &str = "gam";
const AD_SERVER_KEY: &str = "adserver";
const PB_ADSLOT_KEY: &str = "pbadslot";
const GP_ID_KEY: &str = "gpid";
const SK_ADNETWORK_KEY: &str = "skadn";

pub struct Adapter {
    uri: String,
}

impl Adapter {
    /// Go `Builder`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self { uri: endpoint.into() }
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct PubmaticBidExt {
    video: Option<PubmaticBidExtVideo>,
    marketplace: String,
    prebiddealpriority: i64,
    ibv: bool,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct PubmaticBidExtVideo {
    duration: Option<i64>,
}

#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(default)]
struct PubmaticWrapperExt {
    #[serde(rename = "profile", skip_serializing_if = "is_zero")]
    profile_id: i64,
    #[serde(rename = "version", skip_serializing_if = "is_zero")]
    version_id: i64,
    #[serde(rename = "biddercode", skip_serializing_if = "String::is_empty")]
    bidder_code: String,
}

fn is_zero(v: &i64) -> bool {
    *v == 0
}

/// Go `ExtImpBidderPubmatic` (with the embedded `adapters.ExtImpBidder`); the keys are matched
/// case-insensitively by Go, so the object's keys are lower-cased before decoding.
#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpBidderPubmatic {
    bidder: Option<Ext>,
    data: Option<Ext>,
    ae: i64,
    gpid: String,
    skadn: Option<Ext>,
}

/// Go `openrtb_ext.ExtImpPubmatic` (keys lower-cased, see above).
#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpPubmatic {
    publisherid: String,
    adslot: String,
    dctr: String,
    pmzoneid: String,
    wrapper: Option<Ext>,
    keywords: Vec<Option<ExtImpPubmaticKeyVal>>,
    kadfloor: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtImpPubmaticKeyVal {
    key: String,
    value: Vec<String>,
}

#[derive(Serialize, Default)]
struct MarketplaceReqExt {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    allowedbidders: Vec<String>,
}

#[derive(Serialize, Default)]
struct ExtRequestAdServer {
    #[serde(skip_serializing_if = "Option::is_none")]
    wrapper: Option<PubmaticWrapperExt>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    acat: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    marketplace: Option<MarketplaceReqExt>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtRequestIn {
    prebid: ExtRequestPrebid,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtRequestPrebid {
    bidderparams: Option<Ext>,
    aliases: BTreeMap<String, String>,
    alternatebiddercodes: Option<ExtAlternateBidderCodes>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtAlternateBidderCodes {
    enabled: bool,
    bidders: BTreeMap<String, ExtAdapterAlternateBidderCodes>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ExtAdapterAlternateBidderCodes {
    enabled: bool,
    allowedbiddercodes: Option<Vec<String>>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RespExt {
    fledge_auction_configs: Option<BTreeMap<String, Ext>>,
}

#[derive(Serialize)]
struct FledgeAuctionConfig<'a> {
    impid: &'a str,
    config: &'a Ext,
}

/// `Unmarshal` of a raw JSON value into `T`: `null` gives `None` (a nil pointer in Go), an absent
/// or non-object value gives json-iterator's `expect { or n, but found X`.
fn unmarshal_ext_opt<T: serde::de::DeserializeOwned>(ext: Option<&Ext>) -> Result<Option<T>, BidderError> {
    let Some(ext) = ext else {
        return Err(BidderError::FailedToUnmarshal("expect { or n, but found \u{0}".into()));
    };
    if ext.0.is_null() {
        return Ok(None);
    }
    if !ext.0.is_object() {
        let first = ext.to_json().chars().next().unwrap_or('\0');
        return Err(BidderError::FailedToUnmarshal(format!("expect {{ or n, but found {first}")));
    }
    ext.decode().map(Some).map_err(|e| BidderError::FailedToUnmarshal(e.to_string()))
}

fn unmarshal_ext<T: serde::de::DeserializeOwned + Default>(ext: Option<&Ext>) -> Result<T, BidderError> {
    unmarshal_ext_opt(ext).map(Option::unwrap_or_default)
}

/// Lower-cases the top-level keys of an object ext (Go matches struct keys case-insensitively).
fn lowercase_keys(ext: &Ext) -> Ext {
    if !ext.0.is_object() {
        return ext.clone();
    }
    // A raw map keeps the value text (including the key order inside nested values).
    let Ok(entries) = ext.decode::<Vec<(String, Ext)>>().or_else(|_| {
        ext.decode::<BTreeMap<String, Ext>>().map(|m| m.into_iter().collect::<Vec<_>>())
    }) else {
        return ext.clone();
    };
    let mut map: BTreeMap<String, Ext> = BTreeMap::new();
    for (k, v) in entries {
        map.insert(k.to_lowercase(), v);
    }
    let mut out = String::from("{");
    for (i, (k, v)) in map.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&serde_json::to_string(k).unwrap_or_default());
        out.push(':');
        out.push_str(&v.to_json());
    }
    out.push('}');
    Ext::from_slice(out.as_bytes()).unwrap_or_else(|_| ext.clone())
}

/// Go `strconv.FormatFloat(v, 'g', -1, 64)` as `%v` prints a float64.
fn go_fmt_f64(f: f64) -> String {
    if f.is_nan() {
        return "NaN".into();
    }
    if f.is_infinite() {
        return if f > 0.0 { "+Inf".into() } else { "-Inf".into() };
    }
    let abs = f.abs();
    if abs != 0.0 && (abs < 1e-4 || abs >= 1e21) {
        let s = format!("{f:e}");
        let (mant, exp) = s.split_once('e').unwrap_or((&s, "0"));
        let exp: i32 = exp.parse().unwrap_or(0);
        return format!("{mant}e{}{:02}", if exp < 0 { '-' } else { '+' }, exp.abs());
    }
    format!("{f}")
}

fn to_ext<T: Serialize + ?Sized>(v: &T) -> Result<Ext, BidderError> {
    let bytes = crate::go_json::to_vec(v).map_err(|e| BidderError::other(e.to_string()))?;
    Ext::from_slice(&bytes).map_err(|e| BidderError::other(e.to_string()))
}

fn get_display_manager_and_ver(app: &App) -> (String, String) {
    use crate::ext_helpers::ext_get;
    let get = |path: &[&str]| -> String {
        ext_get(app.ext.as_ref(), path)
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .unwrap_or_default()
    };
    let source = get(&["prebid", "source"]);
    if !source.is_empty() {
        let version = get(&["prebid", "version"]);
        if !version.is_empty() {
            return (source, version);
        }
    }
    let source = get(&["source"]);
    if !source.is_empty() {
        let version = get(&["version"]);
        if !version.is_empty() {
            return (source, version);
        }
    }
    (String::new(), String::new())
}

fn get_alternate_bidder_codes_from_request_ext(req_ext: &ExtRequestIn) -> Option<Vec<String>> {
    let abc = req_ext.prebid.alternatebiddercodes.as_ref()?;
    let mut allowed = vec!["pubmatic".to_string()];
    if abc.enabled {
        if let Some(pm) = abc.bidders.get("pubmatic") {
            if pm.enabled {
                match &pm.allowedbiddercodes {
                    None => return Some(vec!["all".into()]),
                    Some(codes) if codes.len() == 1 && codes[0] == "*" => return Some(vec!["all".into()]),
                    Some(codes) => {
                        allowed.extend(codes.iter().cloned());
                        return Some(allowed);
                    }
                }
            }
        }
    }
    Some(allowed)
}

/// Go `extractPubmaticExtFromRequest`.
fn extract_pubmatic_ext_from_request(request: &BidRequest) -> Result<ExtRequestAdServer, BidderError> {
    let mut pm_req_ext = ExtRequestAdServer::default();
    let Some(ext) = &request.ext else {
        return Ok(pm_req_ext);
    };
    let req_ext: ExtRequestIn = match unmarshal_ext_opt(Some(ext)) {
        Ok(v) => v.unwrap_or_default(),
        Err(e) => return Err(BidderError::other(format!("error decoding Request.ext : {e}"))),
    };

    let mut bidder_params: BTreeMap<String, Ext> = BTreeMap::new();
    if let Some(bp) = &req_ext.prebid.bidderparams {
        if let Some(m) = unmarshal_ext_opt::<BTreeMap<String, Ext>>(Some(bp))? {
            bidder_params = m;
        }
    }

    if let Some(wrapper_obj) = bidder_params.get("wrapper") {
        let wrp: Option<PubmaticWrapperExt> = unmarshal_ext_opt(Some(wrapper_obj))?;
        pm_req_ext.wrapper = wrp;
    }
    let wrapper = pm_req_ext.wrapper.get_or_insert_with(PubmaticWrapperExt::default);
    // Always set the bidder code to the default.
    wrapper.bidder_code = BIDDER_PUBMATIC.into();
    // Override the bidder code if an alias exists (Go takes an arbitrary map entry).
    if let Some(alias) = req_ext.prebid.aliases.keys().next() {
        wrapper.bidder_code = alias.clone();
    }

    if let Some(acat_bytes) = bidder_params.get("acat") {
        let acat: Option<Vec<String>> = if acat_bytes.0.is_null() {
            None
        } else {
            Some(
                acat_bytes
                    .decode()
                    .map_err(|e| BidderError::FailedToUnmarshal(e.to_string()))?,
            )
        };
        pm_req_ext.acat = acat.unwrap_or_default().into_iter().map(|a| a.trim().to_string()).collect();
    }

    if let Some(allowed) = get_alternate_bidder_codes_from_request_ext(&req_ext) {
        pm_req_ext.marketplace = Some(MarketplaceReqExt { allowedbidders: allowed });
    }
    Ok(pm_req_ext)
}

/// Go `validateAdSlot`: valid formats are `adslot@WxH`, `adslot` and no adslot.
fn validate_ad_slot(adslot: &str, imp: &mut Imp) -> Result<(), BidderError> {
    let ad_slot_str = adslot.trim();
    if ad_slot_str.is_empty() {
        return Ok(());
    }
    if !ad_slot_str.contains('@') {
        imp.tagid = ad_slot_str.to_string();
        return Ok(());
    }
    let ad_slot: Vec<&str> = ad_slot_str.split('@').collect();
    if ad_slot.len() == 2 && !ad_slot[0].is_empty() && !ad_slot[1].is_empty() {
        imp.tagid = ad_slot[0].trim().to_string();
        let lower = ad_slot[1].to_lowercase();
        let ad_size: Vec<&str> = lower.split('x').collect();
        if ad_size.len() != 2 {
            return Err(BidderError::other(format!("Invalid size provided in adSlot {ad_slot_str}")));
        }
        let width: i64 = ad_size[0]
            .trim()
            .parse()
            .map_err(|_| BidderError::other(format!("Invalid width provided in adSlot {ad_slot_str}")))?;
        let height_str: Vec<&str> = ad_size[1].split(':').collect();
        let height: i64 = height_str[0]
            .trim()
            .parse()
            .map_err(|_| BidderError::other(format!("Invalid height provided in adSlot {ad_slot_str}")))?;
        // In case of video, the size could be derived from the player size.
        if let Some(banner) = &imp.banner {
            imp.banner = Some(assign_banner_width_and_height(banner, width, height));
        }
    } else {
        return Err(BidderError::other(format!("Invalid adSlot {ad_slot_str}")));
    }
    Ok(())
}

fn assign_banner_width_and_height(banner: &Banner, w: i64, h: i64) -> Banner {
    let mut copy = banner.clone();
    copy.w = Some(w);
    copy.h = Some(h);
    copy
}

fn assign_banner_size(banner: &Banner) -> Result<Banner, BidderError> {
    if banner.w.is_some() && banner.h.is_some() {
        return Ok(banner.clone());
    }
    // Go indexes `Format[0]` and panics on an empty list; report an error instead.
    let Some(first) = banner.format.first() else {
        return Err(BidderError::other("banner has no size: no w/h and an empty format list"));
    };
    Ok(assign_banner_width_and_height(banner, first.w, first.h))
}

fn add_keywords_to_ext(keywords: &[Option<ExtImpPubmaticKeyVal>], ext_map: &mut BTreeMap<String, ExtVal>) {
    for key_val in keywords {
        // A `null` entry would be a nil dereference in Go.
        let Some(key_val) = key_val else { continue };
        if key_val.value.is_empty() {
            continue;
        }
        let key = if key_val.key == PM_ZONE_ID_KEY_NAME_OLD { PM_ZONE_ID_KEY_NAME } else { &key_val.key };
        ext_map.insert(key.to_string(), ExtVal::Str(key_val.value.join(",")));
    }
}

/// A value of Go's `map[string]interface{}` imp ext.
enum ExtVal {
    Str(String),
    Int(i64),
    Raw(Ext),
}

fn get_map_from_json(source: &Ext) -> Option<serde_json::Map<String, serde_json::Value>> {
    source.decode::<serde_json::Map<String, serde_json::Value>>().ok()
}

fn populate_first_party_data_imp_attributes(data: &Ext, ext_map: &mut BTreeMap<String, ExtVal>) {
    let Some(data_map) = get_map_from_json(data) else { return };
    populate_ad_unit_key(data, &data_map, ext_map);
    populate_dctr_key(&data_map, ext_map);
}

fn populate_ad_unit_key(
    data: &Ext,
    data_map: &serde_json::Map<String, serde_json::Value>,
    ext_map: &mut BTreeMap<String, ExtVal>,
) {
    use crate::ext_helpers::ext_get;
    let get = |name: &str| -> Option<String> {
        let d = Some(data);
        ext_get(d, &[AD_SERVER_KEY, name]).and_then(|v| v.as_str()).map(str::to_string)
    };
    if get("name").as_deref() == Some(AD_SERVER_GAM) {
        if let Some(adslot) = get("adslot") {
            if !adslot.is_empty() {
                ext_map.insert(IMP_EXT_AD_UNIT_KEY.into(), ExtVal::Str(adslot));
            }
        }
    }
    // imp.ext.dfp_ad_unit_code is not set, then check pbadslot in imp.ext.data.
    if !ext_map.contains_key(IMP_EXT_AD_UNIT_KEY) {
        if let Some(v) = data_map.get(PB_ADSLOT_KEY) {
            if !v.is_null() {
                // Go asserts `.(string)` and panics for any other type; skipped here.
                if let Some(s) = v.as_str() {
                    ext_map.insert(IMP_EXT_AD_UNIT_KEY.into(), ExtVal::Str(s.to_string()));
                }
            }
        }
    }
}

fn get_string_array(array: &[serde_json::Value]) -> Vec<String> {
    let mut out = Vec::with_capacity(array.len());
    for v in array {
        match v.as_str() {
            Some(s) => out.push(s.trim().to_string()),
            None => return Vec::new(),
        }
    }
    out
}

fn populate_dctr_key(
    data_map: &serde_json::Map<String, serde_json::Value>,
    ext_map: &mut BTreeMap<String, ExtVal>,
) {
    let mut dctr = String::new();
    // Append the dctr key if already present in the ext map.
    if let Some(ExtVal::Str(s)) = ext_map.get(DCTR_KEY_NAME) {
        dctr.push_str(s);
    }
    // Go iterates the map in random order; sorted order here.
    for (key, val) in data_map {
        // Ignore the `pbadslot` and `adserver` keys, they are not targeting keys.
        if key == PB_ADSLOT_KEY || key == AD_SERVER_KEY {
            continue;
        }
        // Separate key-value pairs in the dctr string with a pipe.
        if !dctr.is_empty() {
            dctr.push('|');
        }
        let key = key.trim();
        match val {
            serde_json::Value::String(s) => dctr.push_str(&format!("{key}={}", s.trim())),
            serde_json::Value::Number(n) => {
                dctr.push_str(&format!("{key}={}", go_fmt_f64(n.as_f64().unwrap_or(0.0))))
            }
            serde_json::Value::Bool(b) => dctr.push_str(&format!("{key}={b}")),
            serde_json::Value::Array(a) => {
                let arr = get_string_array(a);
                if !arr.is_empty() {
                    dctr.push_str(&format!("{key}={}", arr.join(",")));
                }
            }
            _ => {}
        }
    }
    if !dctr.is_empty() {
        let trimmed = dctr.strip_suffix('|').unwrap_or(&dctr).to_string();
        ext_map.insert(DCTR_KEY_NAME.into(), ExtVal::Str(trimmed));
    }
}

/// Go `json.Marshal(extMap)`: keys sorted, strings HTML-escaped, `RawMessage` written as is.
fn marshal_ext_map(ext_map: &BTreeMap<String, ExtVal>) -> Option<Ext> {
    let mut out = String::from("{");
    for (i, (k, v)) in ext_map.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&String::from_utf8(crate::go_json::to_vec(k).ok()?).ok()?);
        out.push(':');
        match v {
            ExtVal::Str(s) => out.push_str(&String::from_utf8(crate::go_json::to_vec(s).ok()?).ok()?),
            ExtVal::Int(n) => out.push_str(&n.to_string()),
            ExtVal::Raw(e) => out.push_str(&e.to_json()),
        }
    }
    out.push('}');
    Ext::from_slice(out.as_bytes()).ok()
}

/// Go `parseImpressionObject`: gets the imp ready to send to PubMatic.
fn parse_impression_object(
    imp: &mut Imp,
    extract_wrapper_ext_from_imp: bool,
    extract_pub_id_from_imp: bool,
    display_manager: &str,
    display_manager_ver: &str,
) -> Result<(Option<PubmaticWrapperExt>, String), BidderError> {
    let mut wrap_ext: Option<PubmaticWrapperExt> = None;
    let mut pub_id = String::new();

    // PubMatic supports banner, video and native impressions.
    if imp.banner.is_none() && imp.video.is_none() && imp.native.is_none() {
        return Err(BidderError::other(format!(
            "invalid MediaType. PubMatic only supports Banner, Video and Native. Ignoring ImpID={}",
            imp.id
        )));
    }
    imp.audio = None;

    // Populate imp.displaymanager and imp.displaymanagerver if the SDK failed to do it.
    if imp.displaymanager.is_empty()
        && imp.displaymanagerver.is_empty()
        && !display_manager.is_empty()
        && !display_manager_ver.is_empty()
    {
        imp.displaymanager = display_manager.to_string();
        imp.displaymanagerver = display_manager_ver.to_string();
    }

    let imp_ext = imp.ext.as_ref().map(lowercase_keys);
    let bidder_ext: ExtImpBidderPubmatic = unmarshal_ext(imp_ext.as_ref())?;
    let bidder = bidder_ext.bidder.as_ref().map(lowercase_keys);
    let pubmatic_ext: ExtImpPubmatic = unmarshal_ext(bidder.as_ref())?;

    if extract_pub_id_from_imp {
        pub_id = pubmatic_ext.publisherid.trim().to_string();
    }

    // Parse the wrapper extension only once per request.
    if extract_wrapper_ext_from_imp {
        if let Some(wrap) = &pubmatic_ext.wrapper {
            match unmarshal_ext_opt::<PubmaticWrapperExt>(Some(wrap)) {
                Ok(w) => wrap_ext = w,
                Err(e) => {
                    return Err(BidderError::other(format!(
                        "Error in Wrapper Parameters = {}  for ImpID = {} WrapperExt = {}",
                        e.message(),
                        imp.id,
                        wrap.to_json()
                    )))
                }
            }
        }
    }

    validate_ad_slot(pubmatic_ext.adslot.trim(), imp)?;

    if let Some(banner) = &imp.banner {
        imp.banner = Some(assign_banner_size(banner)?);
    }

    if !pubmatic_ext.kadfloor.is_empty() {
        if let Ok(bidfloor) = pubmatic_ext.kadfloor.trim().parse::<f64>() {
            // In case of a valid kadfloor, select the maximum of the original imp.bidfloor and kadfloor.
            imp.bidfloor = if bidfloor.is_nan() || imp.bidfloor.is_nan() {
                f64::NAN
            } else {
                bidfloor.max(imp.bidfloor)
            };
        }
    }

    let mut ext_map: BTreeMap<String, ExtVal> = BTreeMap::new();
    if !pubmatic_ext.keywords.is_empty() {
        add_keywords_to_ext(&pubmatic_ext.keywords, &mut ext_map);
    }
    // Give preference to the direct values of the `dctr` and `pmZoneId` params.
    if !pubmatic_ext.dctr.is_empty() {
        ext_map.insert(DCTR_KEY_NAME.into(), ExtVal::Str(pubmatic_ext.dctr.clone()));
    }
    if !pubmatic_ext.pmzoneid.is_empty() {
        ext_map.insert(PM_ZONE_ID_KEY_NAME.into(), ExtVal::Str(pubmatic_ext.pmzoneid.clone()));
    }

    if let Some(data) = &bidder_ext.data {
        populate_first_party_data_imp_attributes(data, &mut ext_map);
    }
    if bidder_ext.ae != 0 {
        ext_map.insert(AE.into(), ExtVal::Int(bidder_ext.ae));
    }
    if !bidder_ext.gpid.is_empty() {
        ext_map.insert(GP_ID_KEY.into(), ExtVal::Str(bidder_ext.gpid.clone()));
    }
    if let Some(skadn) = bidder_ext.skadn {
        ext_map.insert(SK_ADNETWORK_KEY.into(), ExtVal::Raw(skadn));
    }

    imp.ext = None;
    if !ext_map.is_empty() {
        imp.ext = marshal_ext_map(&ext_map);
    }
    Ok((wrap_ext, pub_id))
}

fn get_media_type_for_bid(bid: &Bid) -> Result<BidType, BidderError> {
    match bid.mtype {
        MarkupType::BANNER => Ok(BidType::Banner),
        MarkupType::VIDEO => Ok(BidType::Video),
        MarkupType::AUDIO => Ok(BidType::Audio),
        MarkupType::NATIVE => Ok(BidType::Native),
        other => Err(BidderError::bad_server_response(format!(
            "failed to parse bid mtype ({}) for impression id {}",
            other.0, bid.impid
        ))),
    }
}

/// Go `getNativeAdm`: moves `bid.adm.native` to `bid.adm`.
fn get_native_adm(adm: &str) -> Result<String, BidderError> {
    let parsed: Option<HashMap<String, Box<serde_json::value::RawValue>>> = serde_json::from_str(adm)
        .map_err(|_| BidderError::other("unable to unmarshal native adm"))?;
    if let Some(native) = parsed.as_ref().and_then(|m| m.get("native")) {
        // jsonparser.Get returns a string value without its quotes.
        let raw = native.get();
        let value = if raw.len() >= 2 && raw.starts_with('"') && raw.ends_with('"') {
            &raw[1..raw.len() - 1]
        } else {
            raw
        };
        return Ok(value.to_string());
    }
    Ok(adm.to_string())
}

impl Bidder for Adapter {
    fn make_requests(
        &self,
        request: &BidRequest,
        _req_info: &ExtraRequestInfo,
    ) -> (Vec<RequestData>, Vec<BidderError>) {
        let mut errs = Vec::with_capacity(request.imp.len());
        let mut req = request.clone();

        let mut pub_id = String::new();
        let mut extract_wrapper_ext_from_imp = true;
        let mut extract_pub_id_from_imp = true;

        let (mut display_manager, mut display_manager_ver) = (String::new(), String::new());
        if let Some(app) = &req.app {
            if app.ext.is_some() {
                (display_manager, display_manager_ver) = get_display_manager_and_ver(app);
            }
        }

        let mut new_req_ext = match extract_pubmatic_ext_from_request(&req) {
            Ok(e) => e,
            Err(e) => return (vec![], vec![e]),
        };
        let mut wrapper_ext = new_req_ext.wrapper.take();
        if let Some(w) = &wrapper_ext {
            if w.profile_id != 0 && w.version_id != 0 {
                extract_wrapper_ext_from_imp = false;
            }
        }

        let mut kept = Vec::with_capacity(req.imp.len());
        for mut imp in std::mem::take(&mut req.imp) {
            match parse_impression_object(
                &mut imp,
                extract_wrapper_ext_from_imp,
                extract_pub_id_from_imp,
                &display_manager,
                &display_manager_ver,
            ) {
                // If the parsing failed, drop the imp and record the error.
                Err(e) => {
                    errs.push(e);
                    continue;
                }
                Ok((wrapper_ext_from_imp, pub_id_from_imp)) => {
                    if extract_wrapper_ext_from_imp {
                        if let Some(from_imp) = wrapper_ext_from_imp {
                            let w = wrapper_ext.get_or_insert_with(PubmaticWrapperExt::default);
                            if w.profile_id == 0 {
                                w.profile_id = from_imp.profile_id;
                            }
                            if w.version_id == 0 {
                                w.version_id = from_imp.version_id;
                            }
                            if w.profile_id != 0 && w.version_id != 0 {
                                extract_wrapper_ext_from_imp = false;
                            }
                        }
                    }
                    if extract_pub_id_from_imp && !pub_id_from_imp.is_empty() {
                        pub_id = pub_id_from_imp;
                        extract_pub_id_from_imp = false;
                    }
                }
            }
            kept.push(imp);
        }
        req.imp = kept;

        // If all the impressions are invalid, the call to the adapter is skipped.
        if req.imp.is_empty() {
            return (vec![], errs);
        }

        new_req_ext.wrapper = wrapper_ext;
        match to_ext(&new_req_ext) {
            Ok(e) => req.ext = Some(e),
            Err(e) => return (vec![], vec![e]),
        }

        if let Some(site) = &mut req.site {
            let mut publisher: Publisher = site.publisher.clone().unwrap_or_default();
            publisher.id = pub_id;
            site.publisher = Some(publisher);
        } else if let Some(app) = &mut req.app {
            let mut publisher: Publisher = app.publisher.clone().unwrap_or_default();
            publisher.id = pub_id;
            app.publisher = Some(publisher);
        }

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
        (
            vec![RequestData {
                method: "POST".into(),
                uri: self.uri.clone(),
                body,
                headers,
                imp_ids: req.imp.iter().map(|i| i.id.clone()).collect(),
            }],
            errs,
        )
    }

    fn make_bids(
        &self,
        _internal_request: &BidRequest,
        _external_request: &RequestData,
        response: &ResponseData,
    ) -> (Option<BidderResponse>, Vec<BidderError>) {
        if response.status_code == 204 {
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
        let mut errs = Vec::new();
        for sb in bid_resp.seatbid {
            for mut bid in sb.bid {
                if bid.cat.len() > 1 {
                    bid.cat.truncate(1);
                }
                let m_type = match get_media_type_for_bid(&bid) {
                    Ok(t) => t,
                    Err(e) => {
                        errs.push(e);
                        continue;
                    }
                };

                let mut meta = None;
                let mut seat = String::new();
                let mut deal_priority = 0;
                let mut duration = 0;
                match unmarshal_ext_opt::<PubmaticBidExt>(bid.ext.as_ref()) {
                    Err(e) => errs.push(e),
                    Ok(None) => {}
                    Ok(Some(bid_ext)) => {
                        seat = bid_ext.marketplace;
                        if bid_ext.prebiddealpriority > 0 {
                            deal_priority = bid_ext.prebiddealpriority as i32;
                        }
                        if let Some(d) = bid_ext.video.and_then(|v| v.duration) {
                            duration = d as i32;
                        }
                        let mut m = ExtBidPrebidMeta { media_type: m_type.as_str().to_string(), ..Default::default() };
                        if bid_ext.ibv {
                            m.media_type = BidType::Video.as_str().to_string();
                        }
                        meta = Some(m);
                    }
                }

                if m_type == BidType::Native {
                    match get_native_adm(&bid.adm) {
                        Ok(adm) => bid.adm = adm,
                        Err(e) => errs.push(e),
                    }
                }

                let mut typed = TypedBid::new(bid, m_type);
                typed.bid_video = Some(ExtBidPrebidVideo { duration, ..Default::default() });
                typed.seat = seat;
                typed.deal_priority = deal_priority;
                typed.bid_meta = meta;
                out.bids.push(typed);
            }
        }
        if !bid_resp.cur.is_empty() {
            out.currency = bid_resp.cur.clone();
        }

        if let Some(ext) = &bid_resp.ext {
            if let Ok(resp_ext) = ext.decode::<RespExt>() {
                if let Some(configs) = resp_ext.fledge_auction_configs {
                    // Go ranges over a map (random order); sorted by imp id here.
                    let list: Vec<FledgeAuctionConfig> = configs
                        .iter()
                        .map(|(imp_id, config)| FledgeAuctionConfig { impid: imp_id, config })
                        .collect();
                    if let Ok(e) = to_ext(&list) {
                        out.fledge_auction_configs = Some(e);
                    }
                }
            }
        }
        (Some(out), errs)
    }
}
