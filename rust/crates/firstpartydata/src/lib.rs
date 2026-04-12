//! First-Party Data (FPD) resolution.
//!
//! Rust port of the Go `firstpartydata` package. Consolidates FPD from the
//! bid request, the global `ext.prebid.data` configuration, and per-bidder
//! `ext.prebid.bidderconfig` overrides into a single [`ResolvedFirstPartyData`]
//! per bidder.
//!
//! Because the Rust port does not yet have strongly-typed OpenRTB wrappers
//! that match the Go implementation one-to-one, this crate operates on
//! `serde_json::Value` trees, which keeps it compatible with whatever OpenRTB
//! representation callers pass in.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use thiserror::Error;

/// Errors produced while resolving first-party data.
#[derive(Debug, Error)]
pub enum FpdError {
    #[error("invalid request ext")]
    BadRequest,
    #[error("invalid first party data ext")]
    BadFpd,
    #[error("incorrect First Party Data for bidder {bidder}: {message}")]
    BadInput { bidder: String, message: String },
}

/// Keys used by the Go implementation for global FPD dictionaries.
pub mod keys {
    pub const SITE: &str = "site";
    pub const APP: &str = "app";
    pub const USER: &str = "user";
    pub const DEVICE: &str = "device";

    pub const USER_DATA: &str = "userData";
    pub const APP_CONTENT_DATA: &str = "appContentData";
    pub const SITE_CONTENT_DATA: &str = "siteContentData";
}

/// Bidder-specific FPD configuration (`ext.prebid.bidderconfig[*].config.ortb2`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BidderFpdConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub site: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<Value>,
}

/// Consolidated per-bidder FPD result.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResolvedFirstPartyData {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub site: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<Value>,
}

/// Deep-merge `patch` onto `target` following RFC 7396 JSON Merge Patch
/// semantics: object keys are merged recursively, arrays and scalars are
/// replaced wholesale, and `null` in the patch deletes keys from `target`.
pub fn merge_patch(target: &mut Value, patch: &Value) {
    match (target, patch) {
        (Value::Object(t), Value::Object(p)) => {
            for (k, v) in p {
                if v.is_null() {
                    t.remove(k);
                } else {
                    merge_patch(t.entry(k.clone()).or_insert(Value::Null), v);
                }
            }
        }
        (t, p) => {
            *t = p.clone();
        }
    }
}

/// Deep-merge `patch` onto `target` without the deletion semantics of
/// [`merge_patch`]: null values in the patch overwrite target values with
/// `null` rather than removing keys. This mirrors the Go
/// `jsonutil.MergeClone` helper used for merging bidder FPD into the Site/
/// App/User objects.
pub fn merge_clone(target: &mut Value, patch: &Value) {
    match (target, patch) {
        (Value::Object(t), Value::Object(p)) => {
            for (k, v) in p {
                merge_clone(t.entry(k.clone()).or_insert(Value::Null), v);
            }
        }
        (t, p) => {
            *t = p.clone();
        }
    }
}

/// Wrap raw FPD data as `{"data": <value>}` ready to merge into an `ext` object.
pub fn build_ext_data(data: &Value) -> Value {
    let mut obj = Map::new();
    obj.insert("data".to_string(), data.clone());
    Value::Object(obj)
}

/// Extract bidder-specific FPD from `req.ext.prebid.bidderconfig`.
///
/// Returns a map keyed by normalized bidder name. Matches the Go
/// `ExtractBidderConfigFPD` helper but operates on a `serde_json::Value`
/// representation of the request ext.
pub fn extract_bidder_config_fpd(
    req_ext: &Value,
) -> Result<HashMap<String, BidderFpdConfig>, FpdError> {
    let mut fpd: HashMap<String, BidderFpdConfig> = HashMap::new();

    let Some(prebid) = req_ext.get("prebid") else {
        return Ok(fpd);
    };
    let Some(bidder_configs) = prebid.get("bidderconfig").and_then(Value::as_array) else {
        return Ok(fpd);
    };

    for bc in bidder_configs {
        let bidders = bc
            .get("bidders")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let ortb2 = bc
            .get("config")
            .and_then(|c| c.get("ortb2"))
            .cloned()
            .unwrap_or(Value::Null);

        for bidder in bidders {
            let Some(name) = bidder.as_str() else { continue };
            let normalized = name.to_ascii_lowercase();

            if fpd.contains_key(&normalized) {
                return Err(FpdError::BadInput {
                    bidder: name.to_string(),
                    message: format!(
                        "multiple First Party Data bidder configs provided for bidder: {name}"
                    ),
                });
            }

            let cfg = BidderFpdConfig {
                site: ortb2.get("site").cloned(),
                app: ortb2.get("app").cloned(),
                user: ortb2.get("user").cloned(),
                device: ortb2.get("device").cloned(),
            };
            fpd.insert(normalized, cfg);
        }
    }
    Ok(fpd)
}

/// Apply global FPD ext data onto a Site/App/User object's `ext` field.
fn apply_global_ext_data(obj: &mut Value, data: &Value) {
    let ext_data = build_ext_data(data);
    match obj.get_mut("ext") {
        Some(existing) if existing.is_object() => merge_patch(existing, &ext_data),
        _ => {
            if let Some(map) = obj.as_object_mut() {
                map.insert("ext".to_string(), ext_data);
            }
        }
    }
}

fn resolve_user(
    fpd_config: Option<&BidderFpdConfig>,
    req_user: Option<&Value>,
    global_fpd: &HashMap<String, Value>,
    open_rtb_global: &HashMap<String, Value>,
) -> Result<Option<Value>, FpdError> {
    let cfg_user = fpd_config.and_then(|c| c.user.as_ref());
    if req_user.is_none() && cfg_user.is_none() {
        return Ok(None);
    }

    let mut new_user = req_user.cloned().unwrap_or_else(|| Value::Object(Map::new()));

    if let Some(data) = global_fpd.get(keys::USER) {
        apply_global_ext_data(&mut new_user, data);
    }
    if let Some(user_data) = open_rtb_global.get(keys::USER_DATA) {
        if let Some(m) = new_user.as_object_mut() {
            m.insert("data".to_string(), user_data.clone());
        }
    }
    if let Some(cfg) = cfg_user {
        merge_clone(&mut new_user, cfg);
    }
    Ok(Some(new_user))
}

fn resolve_site(
    fpd_config: Option<&BidderFpdConfig>,
    req_site: Option<&Value>,
    global_fpd: &HashMap<String, Value>,
    open_rtb_global: &HashMap<String, Value>,
    bidder_name: &str,
) -> Result<Option<Value>, FpdError> {
    let cfg_site = fpd_config.and_then(|c| c.site.as_ref());
    if req_site.is_none() && cfg_site.is_none() {
        return Ok(None);
    }
    if req_site.is_none() && cfg_site.is_some() {
        return Err(FpdError::BadInput {
            bidder: bidder_name.to_string(),
            message: "Site object is not defined in request, but defined in FPD config".to_string(),
        });
    }

    let mut new_site = req_site.cloned().unwrap_or_else(|| Value::Object(Map::new()));

    if let Some(data) = global_fpd.get(keys::SITE) {
        apply_global_ext_data(&mut new_site, data);
    }
    if let Some(content_data) = open_rtb_global.get(keys::SITE_CONTENT_DATA) {
        if let Some(m) = new_site.as_object_mut() {
            let content_entry = m.entry("content".to_string()).or_insert_with(|| Value::Object(Map::new()));
            if let Some(cm) = content_entry.as_object_mut() {
                cm.insert("data".to_string(), content_data.clone());
            }
        }
    }
    if let Some(cfg) = cfg_site {
        merge_clone(&mut new_site, cfg);

        // Re-validate Site: must have id or page.
        let id = new_site.get("id").and_then(Value::as_str).unwrap_or("");
        let page = new_site.get("page").and_then(Value::as_str).unwrap_or("");
        if id.is_empty() && page.is_empty() {
            return Err(FpdError::BadInput {
                bidder: bidder_name.to_string(),
                message: "Site object cannot set empty page if req.site.id is empty".to_string(),
            });
        }
    }
    Ok(Some(new_site))
}

fn resolve_app(
    fpd_config: Option<&BidderFpdConfig>,
    req_app: Option<&Value>,
    global_fpd: &HashMap<String, Value>,
    open_rtb_global: &HashMap<String, Value>,
    bidder_name: &str,
) -> Result<Option<Value>, FpdError> {
    let cfg_app = fpd_config.and_then(|c| c.app.as_ref());
    if req_app.is_none() && cfg_app.is_none() {
        return Ok(None);
    }
    if req_app.is_none() && cfg_app.is_some() {
        return Err(FpdError::BadInput {
            bidder: bidder_name.to_string(),
            message: "App object is not defined in request, but defined in FPD config".to_string(),
        });
    }

    let mut new_app = req_app.cloned().unwrap_or_else(|| Value::Object(Map::new()));

    if let Some(data) = global_fpd.get(keys::APP) {
        apply_global_ext_data(&mut new_app, data);
    }
    if let Some(content_data) = open_rtb_global.get(keys::APP_CONTENT_DATA) {
        if let Some(m) = new_app.as_object_mut() {
            let content_entry = m.entry("content".to_string()).or_insert_with(|| Value::Object(Map::new()));
            if let Some(cm) = content_entry.as_object_mut() {
                cm.insert("data".to_string(), content_data.clone());
            }
        }
    }
    if let Some(cfg) = cfg_app {
        merge_clone(&mut new_app, cfg);
    }
    Ok(Some(new_app))
}

fn resolve_device(
    fpd_config: Option<&BidderFpdConfig>,
    req_device: Option<&Value>,
) -> Result<Option<Value>, FpdError> {
    let cfg_device = fpd_config.and_then(|c| c.device.as_ref());
    if req_device.is_none() && cfg_device.is_none() {
        return Ok(None);
    }
    let mut new_device = req_device.cloned().unwrap_or_else(|| Value::Object(Map::new()));
    if let Some(cfg) = cfg_device {
        merge_clone(&mut new_device, cfg);
    }
    Ok(Some(new_device))
}

/// Consolidate FPD from the different sources and return a [`ResolvedFirstPartyData`]
/// for each bidder that should receive FPD.
///
/// * `bid_request`: the base bid request as a JSON value (should contain
///   top-level `site`/`app`/`user`/`device` keys).
/// * `fpd_bidder_config`: bidder-specific config extracted via
///   [`extract_bidder_config_fpd`].
/// * `global_fpd`: request-level FPD map keyed by `site`/`app`/`user`.
/// * `open_rtb_global_fpd`: OpenRTB-level FPD (e.g. `userData`,
///   `siteContentData`, `appContentData`).
/// * `bidders_with_global_fpd`: if `Some`, only these bidders receive FPD.
///   If `None`, all bidders present in `fpd_bidder_config` are used.
pub fn resolve_fpd(
    bid_request: &Value,
    fpd_bidder_config: &HashMap<String, BidderFpdConfig>,
    global_fpd: &HashMap<String, Value>,
    open_rtb_global_fpd: &HashMap<String, Value>,
    bidders_with_global_fpd: Option<&[String]>,
) -> (HashMap<String, ResolvedFirstPartyData>, Vec<FpdError>) {
    let mut errors = Vec::new();
    let mut resolved = HashMap::new();

    let bidders: Vec<String> = match bidders_with_global_fpd {
        Some(list) => list.iter().map(|b| b.to_ascii_lowercase()).collect(),
        None => fpd_bidder_config.keys().cloned().collect(),
    };

    let req_site = bid_request.get("site");
    let req_app = bid_request.get("app");
    let req_user = bid_request.get("user");
    let req_device = bid_request.get("device");

    for bidder in bidders {
        let cfg = fpd_bidder_config.get(&bidder);
        let mut local_errors = Vec::new();

        let new_user = match resolve_user(cfg, req_user, global_fpd, open_rtb_global_fpd) {
            Ok(v) => v,
            Err(e) => {
                local_errors.push(e);
                None
            }
        };
        let new_app = match resolve_app(cfg, req_app, global_fpd, open_rtb_global_fpd, &bidder) {
            Ok(v) => v,
            Err(e) => {
                local_errors.push(e);
                None
            }
        };
        let new_site = match resolve_site(cfg, req_site, global_fpd, open_rtb_global_fpd, &bidder) {
            Ok(v) => v,
            Err(e) => {
                local_errors.push(e);
                None
            }
        };
        let new_device = match resolve_device(cfg, req_device) {
            Ok(v) => v,
            Err(e) => {
                local_errors.push(e);
                None
            }
        };

        if local_errors.is_empty() {
            resolved.insert(
                bidder,
                ResolvedFirstPartyData {
                    site: new_site,
                    app: new_app,
                    user: new_user,
                    device: new_device,
                },
            );
        } else {
            errors.extend(local_errors);
        }
    }

    (resolved, errors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn merge_patch_deep_merges_objects() {
        let mut target = json!({"a": 1, "nested": {"b": 2, "c": 3}});
        let patch = json!({"nested": {"c": 30, "d": 4}, "e": 5});
        merge_patch(&mut target, &patch);
        assert_eq!(
            target,
            json!({"a": 1, "nested": {"b": 2, "c": 30, "d": 4}, "e": 5})
        );
    }

    #[test]
    fn merge_patch_null_deletes_keys() {
        let mut target = json!({"a": 1, "b": 2});
        let patch = json!({"b": null});
        merge_patch(&mut target, &patch);
        assert_eq!(target, json!({"a": 1}));
    }

    #[test]
    fn build_ext_data_wraps_with_data_key() {
        let v = json!({"foo": "bar"});
        assert_eq!(build_ext_data(&v), json!({"data": {"foo": "bar"}}));
    }

    #[test]
    fn extract_bidder_config_fpd_basic() {
        let req_ext = json!({
            "prebid": {
                "bidderconfig": [
                    {
                        "bidders": ["bidderA"],
                        "config": {
                            "ortb2": {
                                "site": {"name": "overridden"},
                                "user": {"yob": 1990}
                            }
                        }
                    }
                ]
            }
        });
        let fpd = extract_bidder_config_fpd(&req_ext).unwrap();
        assert_eq!(fpd.len(), 1);
        let a = fpd.get("biddera").unwrap();
        assert_eq!(a.site.as_ref().unwrap(), &json!({"name": "overridden"}));
        assert_eq!(a.user.as_ref().unwrap(), &json!({"yob": 1990}));
        assert!(a.app.is_none());
    }

    #[test]
    fn extract_bidder_config_fpd_duplicate_errors() {
        let req_ext = json!({
            "prebid": {
                "bidderconfig": [
                    { "bidders": ["x"], "config": {"ortb2": {"site": {}}} },
                    { "bidders": ["x"], "config": {"ortb2": {"site": {}}} }
                ]
            }
        });
        let err = extract_bidder_config_fpd(&req_ext).unwrap_err();
        assert!(matches!(err, FpdError::BadInput { .. }));
    }

    #[test]
    fn resolve_fpd_merges_site_and_user_overrides() {
        let bid_request = json!({
            "site": {"id": "site1", "name": "original", "page": "http://example.com"},
            "user": {"yob": 1980, "keywords": "k1"}
        });

        let mut fpd_bidder_config = HashMap::new();
        fpd_bidder_config.insert(
            "biddera".to_string(),
            BidderFpdConfig {
                site: Some(json!({"name": "bidder-a-site"})),
                user: Some(json!({"yob": 2000})),
                ..Default::default()
            },
        );

        let (resolved, errors) = resolve_fpd(
            &bid_request,
            &fpd_bidder_config,
            &HashMap::new(),
            &HashMap::new(),
            None,
        );
        assert!(errors.is_empty());
        let r = resolved.get("biddera").expect("biddera resolved");

        let site = r.site.as_ref().unwrap();
        assert_eq!(site.get("name").unwrap(), "bidder-a-site");
        assert_eq!(site.get("id").unwrap(), "site1");

        let user = r.user.as_ref().unwrap();
        assert_eq!(user.get("yob").unwrap(), 2000);
        assert_eq!(user.get("keywords").unwrap(), "k1");
    }

    #[test]
    fn resolve_fpd_applies_global_fpd_to_ext() {
        let bid_request = json!({
            "site": {"id": "s1", "page": "p"},
            "user": {"id": "u1"}
        });

        let mut fpd_bidder_config = HashMap::new();
        fpd_bidder_config.insert("biddera".to_string(), BidderFpdConfig::default());

        let mut global_fpd = HashMap::new();
        global_fpd.insert("site".to_string(), json!({"attr": "s"}));
        global_fpd.insert("user".to_string(), json!({"attr": "u"}));

        let (resolved, errors) = resolve_fpd(
            &bid_request,
            &fpd_bidder_config,
            &global_fpd,
            &HashMap::new(),
            None,
        );
        assert!(errors.is_empty());
        let r = resolved.get("biddera").unwrap();
        assert_eq!(
            r.site.as_ref().unwrap().get("ext").unwrap(),
            &json!({"data": {"attr": "s"}})
        );
        assert_eq!(
            r.user.as_ref().unwrap().get("ext").unwrap(),
            &json!({"data": {"attr": "u"}})
        );
    }

    #[test]
    fn resolve_fpd_site_missing_in_request_with_config_errors() {
        let bid_request = json!({"user": {"id": "u"}});
        let mut fpd_bidder_config = HashMap::new();
        fpd_bidder_config.insert(
            "biddera".to_string(),
            BidderFpdConfig {
                site: Some(json!({"name": "x"})),
                ..Default::default()
            },
        );
        let (resolved, errors) = resolve_fpd(
            &bid_request,
            &fpd_bidder_config,
            &HashMap::new(),
            &HashMap::new(),
            None,
        );
        assert!(resolved.is_empty());
        assert_eq!(errors.len(), 1);
    }
}
