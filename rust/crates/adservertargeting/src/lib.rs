//! Ad server targeting key computation.
//!
//! Rust port of the Go `adservertargeting` package. Computes the
//! `ext.prebid.adservertargeting` keys that should be added to each winning
//! bid's targeting map based on the bid request and response.
//!
//! This crate operates on `serde_json::Value` trees so it does not require
//! strongly typed OpenRTB definitions to be in place. A targeting config
//! entry specifies a `key`, `source` (`static`, `bidrequest`, `bidresponse`)
//! and `value` (either a literal or a dotted JSON path).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

/// Supported targeting data sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DataSource {
    BidRequest,
    Static,
    BidResponse,
}

impl DataSource {
    /// Parse a case-insensitive source string.
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "bidrequest" => Some(DataSource::BidRequest),
            "static" => Some(DataSource::Static),
            "bidresponse" => Some(DataSource::BidResponse),
            _ => None,
        }
    }
}

/// A single ad server targeting configuration entry.
///
/// Mirrors the Go `openrtb_ext.AdServerTarget` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdServerTargeting {
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub value: String,
}

/// A warning produced during targeting processing.
#[derive(Debug, Clone)]
pub struct Warning {
    pub message: String,
}

impl Warning {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// Errors produced during processing. Most problems produce warnings instead,
/// matching the Go behavior that keeps the rest of the request healthy.
#[derive(Debug, Error)]
pub enum TargetingError {
    #[error("{0}")]
    Invalid(String),
}

/// Result of [`process`]: the computed targeting keys per bid (keyed by
/// `impid`), and any warnings produced along the way.
#[derive(Debug, Default, Clone)]
pub struct ProcessResult {
    /// Targeting key/value pairs per bid, keyed by `impid` and then by
    /// ad-server targeting key.
    pub targeting_by_imp: HashMap<String, HashMap<String, String>>,
    pub warnings: Vec<Warning>,
}

const BIDDER_MACRO: &str = "{{BIDDER}}";
const PATH_DELIMITER: char = '.';

/// Validate a list of targeting configs, dropping entries with missing
/// `key`/`value` or an unknown `source`. Matches the Go
/// `validateAdServerTargeting` helper.
pub fn validate(configs: &[AdServerTargeting]) -> (Vec<AdServerTargeting>, Vec<Warning>) {
    let mut validated = Vec::new();
    let mut warnings = Vec::new();
    for (i, t) in configs.iter().enumerate() {
        let mut ok = true;
        if t.key.is_empty() {
            ok = false;
            warnings.push(Warning::new(format!(
                "Key is empty for the ad server targeting object at index {i}"
            )));
        }
        if t.value.is_empty() {
            ok = false;
            warnings.push(Warning::new(format!(
                "Value is empty for the ad server targeting object at index {i}"
            )));
        }
        if DataSource::parse(&t.source).is_none() {
            ok = false;
            warnings.push(Warning::new(format!(
                "Incorrect source for the ad server targeting object at index {i}"
            )));
        }
        if ok {
            validated.push(t.clone());
        }
    }
    (validated, warnings)
}

/// Look up a dotted JSON path inside a [`Value`], returning the resolved
/// value when it is a string or number. Returns `None` if the path is
/// missing or points to a non-scalar type.
pub fn typed_lookup<'a>(data: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = data;
    for segment in path.split(PATH_DELIMITER) {
        if segment.is_empty() {
            return None;
        }
        current = match current {
            Value::Object(m) => m.get(segment)?,
            Value::Array(a) => {
                let idx: usize = segment.parse().ok()?;
                a.get(idx)?
            }
            _ => return None,
        };
    }
    match current {
        Value::String(_) | Value::Number(_) => Some(current),
        _ => None,
    }
}

/// Render a [`Value`] into the string form emitted as a targeting value.
fn value_as_targeting_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        other => other.to_string(),
    }
}

/// Strip a leading prefix from `path`, returning the remaining suffix when
/// the path starts with `prefix`.
fn verify_prefix_and_trim<'a>(path: &'a str, prefix: &str) -> Option<&'a str> {
    path.strip_prefix(prefix)
}

/// Look up a `bidrequest` path, handling the `imp.*` and
/// `ext.prebid.amp.data.*` special cases.
fn resolve_bid_request_value(
    bid_request: &Value,
    path: &str,
    query_params: &HashMap<String, String>,
) -> Result<ResolvedRequestValue, String> {
    // AMP query parameters
    if let Some(param) = verify_prefix_and_trim(path, "ext.prebid.amp.data.") {
        return match query_params.get(param) {
            Some(v) if !v.is_empty() => Ok(ResolvedRequestValue::Single(Value::String(v.clone()))),
            _ => Err(format!("value not found for path: {path}")),
        };
    }

    // Per-imp lookup
    if let Some(imp_path) = verify_prefix_and_trim(path, "imp.") {
        let Some(imps) = bid_request.get("imp").and_then(Value::as_array) else {
            return Err(format!("value not found for path: {path}"));
        };
        let mut by_imp = HashMap::new();
        for imp in imps {
            let Some(id) = imp.get("id").and_then(Value::as_str) else {
                return Err(format!("imp missing id while resolving path: {path}"));
            };
            let val = typed_lookup(imp, imp_path).ok_or_else(|| {
                format!("incorrect value type for path: {path}, value can only be string or number")
            })?;
            by_imp.insert(id.to_string(), value_as_targeting_string(val));
        }
        return Ok(ResolvedRequestValue::PerImp(by_imp));
    }

    // Whole-request lookup.
    let val = typed_lookup(bid_request, path)
        .ok_or_else(|| format!("value not found for path: {path}"))?;
    Ok(ResolvedRequestValue::Single(val.clone()))
}

#[derive(Debug, Clone)]
enum ResolvedRequestValue {
    /// Single scalar that applies to every bid in the response.
    Single(Value),
    /// Per-imp value keyed by `impid`.
    PerImp(HashMap<String, String>),
}

/// Compute the targeting keys for each bid in `bid_response` based on the
/// provided `configs`. Returns per-imp targeting maps and any warnings.
///
/// * `bid_request` – the original bid request (used for `bidrequest` source).
/// * `bid_response` – the response to pull `bidresponse` values from. Should
///   contain a `seatbid[*].bid[*]` shape.
/// * `configs` – the list of targeting configs (raw, will be validated).
/// * `query_params` – AMP query parameters used for `ext.prebid.amp.data.*`.
pub fn process(
    bid_request: &Value,
    bid_response: &Value,
    configs: &[AdServerTargeting],
    query_params: &HashMap<String, String>,
) -> ProcessResult {
    let mut result = ProcessResult::default();
    let (validated, validation_warnings) = validate(configs);
    result.warnings.extend(validation_warnings);

    // Pre-compute request-derived values so we do not walk the request once
    // per bid.
    let mut single_values: HashMap<String, String> = HashMap::new();
    let mut per_imp_values: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut response_configs: Vec<(String, String, bool)> = Vec::new();

    for cfg in &validated {
        let source = DataSource::parse(&cfg.source).expect("validated");
        match source {
            DataSource::Static => {
                single_values.insert(cfg.key.clone(), cfg.value.clone());
            }
            DataSource::BidRequest => {
                match resolve_bid_request_value(bid_request, &cfg.value, query_params) {
                    Ok(ResolvedRequestValue::Single(v)) => {
                        single_values.insert(cfg.key.clone(), value_as_targeting_string(&v));
                    }
                    Ok(ResolvedRequestValue::PerImp(m)) => {
                        per_imp_values.insert(cfg.key.clone(), m);
                    }
                    Err(e) => {
                        result.warnings.push(Warning::new(e));
                    }
                }
            }
            DataSource::BidResponse => {
                let has_macro = cfg.key.to_ascii_uppercase().contains(BIDDER_MACRO);
                response_configs.push((cfg.key.clone(), cfg.value.clone(), has_macro));
            }
        }
    }

    // Walk the response and emit a targeting map per bid.
    let Some(seatbids) = bid_response.get("seatbid").and_then(Value::as_array) else {
        return result;
    };
    for seat in seatbids {
        let bidder = seat
            .get("seat")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let Some(bids) = seat.get("bid").and_then(Value::as_array) else {
            continue;
        };
        for bid in bids {
            let impid = bid
                .get("impid")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let mut targeting: HashMap<String, String> = HashMap::new();

            // Single (static + bidrequest) values.
            for (k, v) in &single_values {
                targeting.insert(k.clone(), v.clone());
            }
            // Per-imp values.
            for (k, by_imp) in &per_imp_values {
                if let Some(v) = by_imp.get(&impid) {
                    targeting.insert(k.clone(), v.clone());
                }
            }
            // BidResponse path lookups.
            for (key, path, has_macro) in &response_configs {
                let effective_key = if *has_macro {
                    key.replace(BIDDER_MACRO, &bidder)
                } else {
                    key.clone()
                };
                if let Some(val) = typed_lookup(bid, path) {
                    targeting.insert(effective_key, value_as_targeting_string(val));
                } else {
                    result
                        .warnings
                        .push(Warning::new(format!("value not found for path: {path}")));
                }
            }

            result
                .targeting_by_imp
                .entry(impid)
                .or_default()
                .extend(targeting);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_data_source_is_case_insensitive() {
        assert_eq!(DataSource::parse("static"), Some(DataSource::Static));
        assert_eq!(DataSource::parse("BidRequest"), Some(DataSource::BidRequest));
        assert_eq!(
            DataSource::parse("BIDRESPONSE"),
            Some(DataSource::BidResponse)
        );
        assert_eq!(DataSource::parse("unknown"), None);
    }

    #[test]
    fn validate_filters_bad_entries() {
        let cfgs = vec![
            AdServerTargeting {
                key: "".into(),
                source: "static".into(),
                value: "v".into(),
            },
            AdServerTargeting {
                key: "k".into(),
                source: "static".into(),
                value: "".into(),
            },
            AdServerTargeting {
                key: "k".into(),
                source: "not-a-source".into(),
                value: "v".into(),
            },
            AdServerTargeting {
                key: "ok".into(),
                source: "static".into(),
                value: "v".into(),
            },
        ];
        let (valid, warnings) = validate(&cfgs);
        assert_eq!(valid.len(), 1);
        assert_eq!(valid[0].key, "ok");
        assert_eq!(warnings.len(), 3);
    }

    #[test]
    fn typed_lookup_finds_nested_scalar() {
        let v = json!({"a": {"b": "hello"}});
        assert_eq!(typed_lookup(&v, "a.b").unwrap(), &json!("hello"));
        assert!(typed_lookup(&v, "a").is_none()); // object, not scalar
        assert!(typed_lookup(&v, "a.c").is_none());
    }

    #[test]
    fn process_static_source() {
        let req = json!({});
        let resp = json!({
            "seatbid": [
                {
                    "seat": "bidderA",
                    "bid": [{"impid": "imp1"}]
                }
            ]
        });
        let cfgs = vec![AdServerTargeting {
            key: "hb_static".into(),
            source: "static".into(),
            value: "the_value".into(),
        }];
        let res = process(&req, &resp, &cfgs, &HashMap::new());
        assert!(res.warnings.is_empty());
        assert_eq!(
            res.targeting_by_imp
                .get("imp1")
                .and_then(|m| m.get("hb_static"))
                .unwrap(),
            "the_value"
        );
    }

    #[test]
    fn process_bid_request_single_value() {
        let req = json!({"site": {"name": "example"}});
        let resp = json!({
            "seatbid": [
                {"seat": "bidderA", "bid": [{"impid": "i1"}, {"impid": "i2"}]}
            ]
        });
        let cfgs = vec![AdServerTargeting {
            key: "hb_site".into(),
            source: "bidrequest".into(),
            value: "site.name".into(),
        }];
        let res = process(&req, &resp, &cfgs, &HashMap::new());
        assert!(res.warnings.is_empty(), "warnings: {:?}", res.warnings);
        assert_eq!(
            res.targeting_by_imp.get("i1").unwrap().get("hb_site").unwrap(),
            "example"
        );
        assert_eq!(
            res.targeting_by_imp.get("i2").unwrap().get("hb_site").unwrap(),
            "example"
        );
    }

    #[test]
    fn process_bid_request_per_imp_value() {
        let req = json!({
            "imp": [
                {"id": "i1", "tagid": "t-one"},
                {"id": "i2", "tagid": "t-two"}
            ]
        });
        let resp = json!({
            "seatbid": [
                {"seat": "bidderA", "bid": [{"impid": "i1"}, {"impid": "i2"}]}
            ]
        });
        let cfgs = vec![AdServerTargeting {
            key: "hb_tag".into(),
            source: "bidrequest".into(),
            value: "imp.tagid".into(),
        }];
        let res = process(&req, &resp, &cfgs, &HashMap::new());
        assert!(res.warnings.is_empty(), "warnings: {:?}", res.warnings);
        assert_eq!(
            res.targeting_by_imp.get("i1").unwrap().get("hb_tag").unwrap(),
            "t-one"
        );
        assert_eq!(
            res.targeting_by_imp.get("i2").unwrap().get("hb_tag").unwrap(),
            "t-two"
        );
    }

    #[test]
    fn process_bid_response_path_with_bidder_macro() {
        let req = json!({});
        let resp = json!({
            "seatbid": [
                {
                    "seat": "bidderA",
                    "bid": [{"impid": "i1", "adid": "42"}]
                }
            ]
        });
        let cfgs = vec![AdServerTargeting {
            key: "hb_adid_{{BIDDER}}".into(),
            source: "bidresponse".into(),
            value: "adid".into(),
        }];
        let res = process(&req, &resp, &cfgs, &HashMap::new());
        assert!(res.warnings.is_empty(), "warnings: {:?}", res.warnings);
        let imp = res.targeting_by_imp.get("i1").unwrap();
        assert_eq!(imp.get("hb_adid_bidderA").unwrap(), "42");
    }

    #[test]
    fn process_amp_query_param() {
        let req = json!({});
        let resp = json!({
            "seatbid": [
                {"seat": "bidderA", "bid": [{"impid": "i1"}]}
            ]
        });
        let cfgs = vec![AdServerTargeting {
            key: "hb_amp".into(),
            source: "bidrequest".into(),
            value: "ext.prebid.amp.data.country".into(),
        }];
        let mut qp = HashMap::new();
        qp.insert("country".to_string(), "US".to_string());
        let res = process(&req, &resp, &cfgs, &qp);
        assert!(res.warnings.is_empty());
        assert_eq!(
            res.targeting_by_imp.get("i1").unwrap().get("hb_amp").unwrap(),
            "US"
        );
    }

    #[test]
    fn process_missing_bid_request_path_warns() {
        let req = json!({});
        let resp = json!({
            "seatbid": [{"seat": "bA", "bid": [{"impid": "i1"}]}]
        });
        let cfgs = vec![AdServerTargeting {
            key: "hb_missing".into(),
            source: "bidrequest".into(),
            value: "site.name".into(),
        }];
        let res = process(&req, &resp, &cfgs, &HashMap::new());
        assert_eq!(res.warnings.len(), 1);
        assert!(res.targeting_by_imp.get("i1").map_or(true, |m| !m.contains_key("hb_missing")));
    }
}
