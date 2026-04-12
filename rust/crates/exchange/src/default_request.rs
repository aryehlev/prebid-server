//! Default request loading and merging.
//!
//! Mirrors Go `router/router.go` `readDefaultRequest()` and the JSON
//! merge-patch application in `endpoints/openrtb2/auction.go`.
//!
//! Allows publishers to define a default BidRequest JSON template that
//! gets merged with every incoming request using RFC 7396 JSON Merge Patch.

use serde_json::Value;
use std::path::Path;
use crate::json_merge::merge_patch;

// ---------------------------------------------------------------------------
// DefaultRequestConfig — mirrors Go config.DefReqConfig
// ---------------------------------------------------------------------------

/// Configuration for loading a default bid request template.
#[derive(Debug, Clone, Default)]
pub struct DefaultRequestConfig {
    /// Source type. Currently only "file" is supported, matching Go.
    pub source_type: DefaultRequestType,
    /// File path when `source_type` is `File`.
    pub file_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum DefaultRequestType {
    #[default]
    None,
    File,
}

// ---------------------------------------------------------------------------
// Loading — mirrors Go readDefaultRequest + readDefaultRequestFromFile
// ---------------------------------------------------------------------------

/// Errors from loading a default request template.
#[derive(Debug, thiserror::Error)]
pub enum DefaultRequestError {
    #[error("failed to read default request from file {path}: {source}")]
    ReadFile {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid JSON in default request from file {path}: {source}")]
    InvalidJson {
        path: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("default request JSON is not an object (must be a valid BidRequest shape)")]
    NotAnObject,
}

/// Load the default request JSON template.
///
/// Returns `None` (an empty value) if no default request is configured.
/// Mirrors Go `readDefaultRequest`.
pub fn load_default_request(cfg: &DefaultRequestConfig) -> Result<Option<Value>, DefaultRequestError> {
    match cfg.source_type {
        DefaultRequestType::None => Ok(None),
        DefaultRequestType::File => {
            let path = match &cfg.file_name {
                Some(p) if !p.is_empty() => p,
                _ => return Ok(None),
            };
            load_from_file(path).map(Some)
        }
    }
}

/// Load and validate a default request from a file path.
/// Mirrors Go `readDefaultRequestFromFile`.
pub fn load_from_file(path: &str) -> Result<Value, DefaultRequestError> {
    let content = std::fs::read_to_string(Path::new(path)).map_err(|e| {
        DefaultRequestError::ReadFile {
            path: path.to_string(),
            source: e,
        }
    })?;

    let value: Value = serde_json::from_str(&content).map_err(|e| {
        DefaultRequestError::InvalidJson {
            path: path.to_string(),
            source: e,
        }
    })?;

    if !value.is_object() {
        return Err(DefaultRequestError::NotAnObject);
    }

    Ok(value)
}

// ---------------------------------------------------------------------------
// Merging — applies default request to incoming request
// ---------------------------------------------------------------------------

/// Merge a default request template with an incoming request.
///
/// The incoming request takes precedence — it is the patch applied on top
/// of the default. Mirrors Go `jsonpatch.MergePatch(defReqJSON, resolvedRequest)`.
pub fn apply_default_request(default: &Value, incoming: &Value) -> Value {
    merge_patch(default, incoming)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;

    #[test]
    fn test_load_default_request_none_type() {
        let cfg = DefaultRequestConfig::default();
        assert!(load_default_request(&cfg).unwrap().is_none());
    }

    #[test]
    fn test_load_default_request_no_filename() {
        let cfg = DefaultRequestConfig {
            source_type: DefaultRequestType::File,
            file_name: None,
        };
        assert!(load_default_request(&cfg).unwrap().is_none());
    }

    #[test]
    fn test_load_default_request_empty_filename() {
        let cfg = DefaultRequestConfig {
            source_type: DefaultRequestType::File,
            file_name: Some(String::new()),
        };
        assert!(load_default_request(&cfg).unwrap().is_none());
    }

    #[test]
    fn test_load_from_file() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        writeln!(f, r#"{{"tmax": 3000, "cur": ["USD"]}}"#).unwrap();
        let path = f.path().to_str().unwrap().to_string();

        let cfg = DefaultRequestConfig {
            source_type: DefaultRequestType::File,
            file_name: Some(path),
        };

        let loaded = load_default_request(&cfg).unwrap().unwrap();
        assert_eq!(loaded["tmax"], json!(3000));
        assert_eq!(loaded["cur"], json!(["USD"]));
    }

    #[test]
    fn test_load_from_file_invalid_json() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        writeln!(f, "{{bad json").unwrap();
        let path = f.path().to_str().unwrap().to_string();

        let result = load_from_file(&path);
        assert!(matches!(result, Err(DefaultRequestError::InvalidJson { .. })));
    }

    #[test]
    fn test_load_from_file_not_an_object() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        writeln!(f, "[1, 2, 3]").unwrap();
        let path = f.path().to_str().unwrap().to_string();

        let result = load_from_file(&path);
        assert!(matches!(result, Err(DefaultRequestError::NotAnObject)));
    }

    #[test]
    fn test_load_from_file_not_found() {
        let result = load_from_file("/nonexistent/path/does/not/exist.json");
        assert!(matches!(result, Err(DefaultRequestError::ReadFile { .. })));
    }

    #[test]
    fn test_apply_default_request_merge() {
        let default = json!({
            "tmax": 3000,
            "cur": ["USD"],
            "regs": {"gdpr": 1}
        });
        let incoming = json!({
            "tmax": 5000,
            "imp": [{"id": "1"}]
        });

        let merged = apply_default_request(&default, &incoming);

        // Incoming overrides default tmax
        assert_eq!(merged["tmax"], json!(5000));
        // Default cur preserved
        assert_eq!(merged["cur"], json!(["USD"]));
        // Default regs preserved
        assert_eq!(merged["regs"]["gdpr"], json!(1));
        // Incoming imp added
        assert_eq!(merged["imp"], json!([{"id": "1"}]));
    }

    #[test]
    fn test_apply_default_request_nested_override() {
        let default = json!({
            "ext": {"prebid": {"debug": true, "cache": {"bids": {}}}}
        });
        let incoming = json!({
            "ext": {"prebid": {"debug": false}}
        });

        let merged = apply_default_request(&default, &incoming);

        // Debug overridden
        assert_eq!(merged["ext"]["prebid"]["debug"], json!(false));
        // Cache preserved from default
        assert!(merged["ext"]["prebid"]["cache"].is_object());
    }
}
