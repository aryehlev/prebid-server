//! First Party Data (FPD) handling for prebid-server.
//!
//! This module provides functionality to extract and apply per-bidder first party data
//! configuration, merging bidder-specific site/app/user overrides into individual
//! bid request copies.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use openrtb::BidRequest;

/// Per-bidder first party data configuration.
///
/// Contains optional JSON overrides for site, app, and user objects that will be
/// deep-merged into the bid request before sending to a specific bidder.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FpdConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub site: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<Value>,
}

/// Deep-merges two JSON values recursively.
///
/// When both `target` and `source` are objects, their keys are merged recursively.
/// For all other cases (arrays, scalars, nulls), `source` replaces `target`.
/// This means arrays are replaced wholesale, not concatenated.
pub fn merge_json_objects(target: &Value, source: &Value) -> Value {
    match (target, source) {
        (Value::Object(target_map), Value::Object(source_map)) => {
            let mut merged = target_map.clone();
            for (key, source_val) in source_map {
                let merged_val = match merged.get(key) {
                    Some(target_val) => merge_json_objects(target_val, source_val),
                    None => source_val.clone(),
                };
                merged.insert(key.clone(), merged_val);
            }
            Value::Object(merged)
        }
        // Source wins for non-object types (arrays replaced, not concatenated).
        (_, source_val) => source_val.clone(),
    }
}

/// Extracts bidder-specific FPD from the request's `ext.prebid.bidderconfig` or
/// `ext.prebid.data.bidders` structure.
///
/// Looks into `request.ext.prebid.data.bidders.<bidder_name>` for per-bidder overrides
/// and returns an `FpdConfig` with site/app/user JSON values if present.
pub fn extract_fpd_for_bidder(request: &BidRequest, bidder_name: &str) -> FpdConfig {
    let ext = match &request.ext {
        Some(ext) => ext,
        None => return FpdConfig::default(),
    };

    let prebid = match ext.get("prebid") {
        Some(p) => p,
        None => return FpdConfig::default(),
    };

    // Check ext.prebid.bidderconfig for per-bidder FPD.
    // Format: [{ "bidders": ["bidderA"], "config": { "ortb2": { "site": {...}, "app": {...}, "user": {...} } } }]
    if let Some(bidderconfigs) = prebid.get("bidderconfig").and_then(|v| v.as_array()) {
        for entry in bidderconfigs {
            let empty_vec = Vec::new();
            let bidders_arr = entry
                .get("bidders")
                .and_then(|v| v.as_array())
                .unwrap_or(&empty_vec);

            let matches = bidders_arr
                .iter()
                .filter_map(|v| v.as_str())
                .any(|b| b == bidder_name || b == "*");

            if matches {
                if let Some(config) = entry.get("config").and_then(|c| c.get("ortb2")) {
                    return FpdConfig {
                        site: config.get("site").cloned(),
                        app: config.get("app").cloned(),
                        user: config.get("user").cloned(),
                    };
                }
            }
        }
    }

    // Fallback: check ext.prebid.data.bidders.<bidder_name>
    if let Some(bidder_data) = prebid
        .get("data")
        .and_then(|d| d.get("bidders"))
        .and_then(|b| b.get(bidder_name))
    {
        return FpdConfig {
            site: bidder_data.get("site").cloned(),
            app: bidder_data.get("app").cloned(),
            user: bidder_data.get("user").cloned(),
        };
    }

    FpdConfig::default()
}

/// Applies first party data to a cloned bid request.
///
/// Deep-merges the FPD config's site/app/user JSON into the corresponding request
/// objects. Bidder FPD values take precedence over existing request values.
pub fn apply_fpd_to_request(request: &BidRequest, fpd: &FpdConfig) -> BidRequest {
    let mut req = request.clone();

    if let Some(ref site_fpd) = fpd.site {
        let base = match &req.site {
            Some(site) => serde_json::to_value(site).unwrap_or(Value::Object(Default::default())),
            None => Value::Object(Default::default()),
        };
        let merged = merge_json_objects(&base, site_fpd);
        if let Ok(site) = serde_json::from_value(merged) {
            req.site = Some(site);
        }
    }

    if let Some(ref app_fpd) = fpd.app {
        let base = match &req.app {
            Some(app) => serde_json::to_value(app).unwrap_or(Value::Object(Default::default())),
            None => Value::Object(Default::default()),
        };
        let merged = merge_json_objects(&base, app_fpd);
        if let Ok(app) = serde_json::from_value(merged) {
            req.app = Some(app);
        }
    }

    if let Some(ref user_fpd) = fpd.user {
        let base = match &req.user {
            Some(user) => serde_json::to_value(user).unwrap_or(Value::Object(Default::default())),
            None => Value::Object(Default::default()),
        };
        let merged = merge_json_objects(&base, user_fpd);
        if let Ok(user) = serde_json::from_value(merged) {
            req.user = Some(user);
        }
    }

    req
}

/// Main entry point: resolves per-bidder FPD and returns a map from bidder name
/// to a modified `BidRequest` with that bidder's FPD applied.
///
/// If `fpd_config` is provided, those configs are used directly. Otherwise,
/// bidder-specific FPD is extracted from the request's ext.
pub fn resolve_fpd(
    request: &BidRequest,
    fpd_config: &HashMap<String, FpdConfig>,
) -> HashMap<String, BidRequest> {
    let mut result = HashMap::new();

    for (bidder_name, config) in fpd_config {
        let bidder_req = apply_fpd_to_request(request, config);
        result.insert(bidder_name.clone(), bidder_req);
    }

    result
}

/// Convenience: extracts FPD from the request for a list of bidders, then applies it.
///
/// Returns a map from bidder name to a modified `BidRequest`.
pub fn resolve_fpd_for_bidders(
    request: &BidRequest,
    bidder_names: &[&str],
) -> HashMap<String, BidRequest> {
    let mut fpd_config = HashMap::new();
    for &bidder in bidder_names {
        let config = extract_fpd_for_bidder(request, bidder);
        fpd_config.insert(bidder.to_string(), config);
    }
    resolve_fpd(request, &fpd_config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_merge_json_objects_both_objects() {
        let target = json!({"a": 1, "b": {"c": 2, "d": 3}});
        let source = json!({"b": {"c": 99, "e": 5}, "f": 6});
        let result = merge_json_objects(&target, &source);
        assert_eq!(result, json!({"a": 1, "b": {"c": 99, "d": 3, "e": 5}, "f": 6}));
    }

    #[test]
    fn test_merge_json_objects_source_replaces_non_object() {
        let target = json!({"a": [1, 2, 3]});
        let source = json!({"a": [4, 5]});
        let result = merge_json_objects(&target, &source);
        // Arrays are replaced, not concatenated.
        assert_eq!(result, json!({"a": [4, 5]}));
    }

    #[test]
    fn test_merge_json_objects_source_scalar_wins() {
        let target = json!({"a": "old"});
        let source = json!({"a": "new"});
        let result = merge_json_objects(&target, &source);
        assert_eq!(result, json!({"a": "new"}));
    }

    #[test]
    fn test_merge_json_objects_non_object_target() {
        let target = json!("string");
        let source = json!({"a": 1});
        let result = merge_json_objects(&target, &source);
        // Source replaces target when target is not an object.
        assert_eq!(result, json!({"a": 1}));
    }

    #[test]
    fn test_merge_json_objects_nested_deep() {
        let target = json!({"a": {"b": {"c": 1, "d": 2}}});
        let source = json!({"a": {"b": {"c": 99}}});
        let result = merge_json_objects(&target, &source);
        assert_eq!(result, json!({"a": {"b": {"c": 99, "d": 2}}}));
    }

    #[test]
    fn test_extract_fpd_for_bidder_no_ext() {
        let req = BidRequest::default();
        let fpd = extract_fpd_for_bidder(&req, "appnexus");
        assert!(fpd.site.is_none());
        assert!(fpd.app.is_none());
        assert!(fpd.user.is_none());
    }

    #[test]
    fn test_extract_fpd_for_bidder_from_bidderconfig() {
        let req = BidRequest {
            id: "test".to_string(),
            ext: Some(json!({
                "prebid": {
                    "bidderconfig": [
                        {
                            "bidders": ["appnexus"],
                            "config": {
                                "ortb2": {
                                    "site": {"name": "fpd-site"},
                                    "user": {"keywords": "fpd-kw"}
                                }
                            }
                        }
                    ]
                }
            })),
            ..Default::default()
        };

        let fpd = extract_fpd_for_bidder(&req, "appnexus");
        assert_eq!(fpd.site, Some(json!({"name": "fpd-site"})));
        assert_eq!(fpd.user, Some(json!({"keywords": "fpd-kw"})));
        assert!(fpd.app.is_none());
    }

    #[test]
    fn test_extract_fpd_for_bidder_wildcard() {
        let req = BidRequest {
            id: "test".to_string(),
            ext: Some(json!({
                "prebid": {
                    "bidderconfig": [
                        {
                            "bidders": ["*"],
                            "config": {
                                "ortb2": {
                                    "site": {"domain": "example.com"}
                                }
                            }
                        }
                    ]
                }
            })),
            ..Default::default()
        };

        let fpd = extract_fpd_for_bidder(&req, "rubicon");
        assert_eq!(fpd.site, Some(json!({"domain": "example.com"})));
    }

    #[test]
    fn test_extract_fpd_for_bidder_no_match() {
        let req = BidRequest {
            id: "test".to_string(),
            ext: Some(json!({
                "prebid": {
                    "bidderconfig": [
                        {
                            "bidders": ["appnexus"],
                            "config": {
                                "ortb2": {
                                    "site": {"name": "fpd-site"}
                                }
                            }
                        }
                    ]
                }
            })),
            ..Default::default()
        };

        let fpd = extract_fpd_for_bidder(&req, "rubicon");
        assert!(fpd.site.is_none());
    }

    #[test]
    fn test_extract_fpd_for_bidder_from_data_bidders() {
        let req = BidRequest {
            id: "test".to_string(),
            ext: Some(json!({
                "prebid": {
                    "data": {
                        "bidders": {
                            "rubicon": {
                                "site": {"page": "https://example.com"},
                                "user": {"gender": "M"}
                            }
                        }
                    }
                }
            })),
            ..Default::default()
        };

        let fpd = extract_fpd_for_bidder(&req, "rubicon");
        assert_eq!(fpd.site, Some(json!({"page": "https://example.com"})));
        assert_eq!(fpd.user, Some(json!({"gender": "M"})));
        assert!(fpd.app.is_none());
    }

    #[test]
    fn test_apply_fpd_to_request_site_merge() {
        let req = BidRequest {
            id: "test".to_string(),
            site: Some(openrtb::Site {
                name: Some("original".to_string()),
                domain: Some("example.com".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        let fpd = FpdConfig {
            site: Some(json!({"name": "overridden", "keywords": "kw1"})),
            app: None,
            user: None,
        };

        let result = apply_fpd_to_request(&req, &fpd);
        let site = result.site.unwrap();
        assert_eq!(site.name, Some("overridden".to_string()));
        assert_eq!(site.domain, Some("example.com".to_string()));
        assert_eq!(site.keywords, Some("kw1".to_string()));
    }

    #[test]
    fn test_apply_fpd_to_request_creates_site() {
        let req = BidRequest {
            id: "test".to_string(),
            ..Default::default()
        };

        let fpd = FpdConfig {
            site: Some(json!({"name": "new-site"})),
            app: None,
            user: None,
        };

        let result = apply_fpd_to_request(&req, &fpd);
        let site = result.site.unwrap();
        assert_eq!(site.name, Some("new-site".to_string()));
    }

    #[test]
    fn test_apply_fpd_to_request_user_merge() {
        let req = BidRequest {
            id: "test".to_string(),
            user: Some(openrtb::User {
                id: Some("user123".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        let fpd = FpdConfig {
            site: None,
            app: None,
            user: Some(json!({"keywords": "interests", "gender": "F"})),
        };

        let result = apply_fpd_to_request(&req, &fpd);
        let user = result.user.unwrap();
        assert_eq!(user.id, Some("user123".to_string()));
        assert_eq!(user.keywords, Some("interests".to_string()));
        assert_eq!(user.gender, Some("F".to_string()));
    }

    #[test]
    fn test_apply_fpd_to_request_app_merge() {
        let req = BidRequest {
            id: "test".to_string(),
            app: Some(openrtb::App {
                name: Some("MyApp".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        let fpd = FpdConfig {
            site: None,
            app: Some(json!({"bundle": "com.example.app", "keywords": "games"})),
            user: None,
        };

        let result = apply_fpd_to_request(&req, &fpd);
        let app = result.app.unwrap();
        assert_eq!(app.name, Some("MyApp".to_string()));
        assert_eq!(app.bundle, Some("com.example.app".to_string()));
        assert_eq!(app.keywords, Some("games".to_string()));
    }

    #[test]
    fn test_apply_fpd_empty_config() {
        let req = BidRequest {
            id: "test".to_string(),
            site: Some(openrtb::Site {
                name: Some("original".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        let fpd = FpdConfig::default();
        let result = apply_fpd_to_request(&req, &fpd);
        assert_eq!(result.site.unwrap().name, Some("original".to_string()));
    }

    #[test]
    fn test_resolve_fpd_multiple_bidders() {
        let req = BidRequest {
            id: "test".to_string(),
            site: Some(openrtb::Site {
                domain: Some("example.com".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        let mut configs = HashMap::new();
        configs.insert(
            "appnexus".to_string(),
            FpdConfig {
                site: Some(json!({"name": "appnexus-site"})),
                app: None,
                user: None,
            },
        );
        configs.insert(
            "rubicon".to_string(),
            FpdConfig {
                site: Some(json!({"name": "rubicon-site"})),
                app: None,
                user: Some(json!({"keywords": "rubicon-kw"})),
            },
        );

        let result = resolve_fpd(&req, &configs);
        assert_eq!(result.len(), 2);

        let an_req = &result["appnexus"];
        assert_eq!(an_req.site.as_ref().unwrap().name, Some("appnexus-site".to_string()));
        assert_eq!(
            an_req.site.as_ref().unwrap().domain,
            Some("example.com".to_string())
        );
        assert!(an_req.user.is_none());

        let rb_req = &result["rubicon"];
        assert_eq!(rb_req.site.as_ref().unwrap().name, Some("rubicon-site".to_string()));
        assert_eq!(
            rb_req.user.as_ref().unwrap().keywords,
            Some("rubicon-kw".to_string())
        );
    }

    #[test]
    fn test_resolve_fpd_for_bidders_integration() {
        let req = BidRequest {
            id: "test".to_string(),
            site: Some(openrtb::Site {
                domain: Some("example.com".to_string()),
                ..Default::default()
            }),
            ext: Some(json!({
                "prebid": {
                    "bidderconfig": [
                        {
                            "bidders": ["appnexus"],
                            "config": {
                                "ortb2": {
                                    "site": {"name": "fpd-appnexus"}
                                }
                            }
                        },
                        {
                            "bidders": ["*"],
                            "config": {
                                "ortb2": {
                                    "user": {"keywords": "global-kw"}
                                }
                            }
                        }
                    ]
                }
            })),
            ..Default::default()
        };

        let result = resolve_fpd_for_bidders(&req, &["appnexus", "rubicon"]);
        assert_eq!(result.len(), 2);

        // appnexus matches the first bidderconfig entry (specific match)
        let an_req = &result["appnexus"];
        assert_eq!(an_req.site.as_ref().unwrap().name, Some("fpd-appnexus".to_string()));
        assert_eq!(
            an_req.site.as_ref().unwrap().domain,
            Some("example.com".to_string())
        );

        // rubicon matches the wildcard entry
        let rb_req = &result["rubicon"];
        assert_eq!(
            rb_req.user.as_ref().unwrap().keywords,
            Some("global-kw".to_string())
        );
    }

    #[test]
    fn test_apply_fpd_site_ext_deep_merge() {
        let req = BidRequest {
            id: "test".to_string(),
            site: Some(openrtb::Site {
                ext: Some(json!({"data": {"key1": "val1"}, "other": true})),
                ..Default::default()
            }),
            ..Default::default()
        };

        let fpd = FpdConfig {
            site: Some(json!({"ext": {"data": {"key2": "val2"}}})),
            app: None,
            user: None,
        };

        let result = apply_fpd_to_request(&req, &fpd);
        let site_ext = result.site.unwrap().ext.unwrap();
        // Deep merged: key1 preserved, key2 added, other preserved.
        assert_eq!(site_ext["data"]["key1"], json!("val1"));
        assert_eq!(site_ext["data"]["key2"], json!("val2"));
        assert_eq!(site_ext["other"], json!(true));
    }
}
