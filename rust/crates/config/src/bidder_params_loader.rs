//! Static bidder-params JSON schema loaders.
//!
//! Loads per-bidder `*.json` schema files from a directory and compiles
//! them into [`jsonschema::JSONSchema`] validators that can be used to
//! check adapter-specific `imp.ext.prebid.bidder.<name>` payloads.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use jsonschema::JSONSchema;
use serde_json::Value;

use crate::bidder_info_loader::ConfigError;

/// Walk a directory of `*.json` bidder params schema files and compile each
/// one into a [`JSONSchema`] validator. The returned map is keyed by the
/// filename stem (e.g. `appnexus.json` -> `"appnexus"`).
pub fn load_bidder_params_from_dir(
    path: &Path,
) -> Result<HashMap<String, JSONSchema>, ConfigError> {
    if !path.is_dir() {
        return Err(ConfigError::NotADirectory {
            path: path.display().to_string(),
        });
    }

    let mut out = HashMap::new();

    let entries = fs::read_dir(path).map_err(|e| ConfigError::Io {
        path: path.display().to_string(),
        source: e,
    })?;

    for entry in entries {
        let entry = entry.map_err(|e| ConfigError::Io {
            path: path.display().to_string(),
            source: e,
        })?;
        let entry_path = entry.path();

        let ext = entry_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        if ext != "json" {
            continue;
        }

        let stem = match entry_path.file_stem().and_then(|s| s.to_str()) {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => continue,
        };

        let contents = fs::read_to_string(&entry_path).map_err(|e| ConfigError::Io {
            path: entry_path.display().to_string(),
            source: e,
        })?;

        let json: Value = serde_json::from_str(&contents).map_err(|e| ConfigError::Json {
            bidder: stem.clone(),
            source: e,
        })?;

        // `JSONSchema::compile` returns an owned `JSONSchema` (its only
        // lifetime-parameterised output is the error type, which we convert
        // to an owned `String` here so nothing borrows from `json`).
        let compiled = JSONSchema::compile(&json).map_err(|e| ConfigError::SchemaCompile {
            bidder: stem.clone(),
            message: e.to_string(),
        })?;

        out.insert(stem, compiled);
    }

    Ok(out)
}

/// Validate a JSON value against a compiled bidder params schema and return
/// a list of human-readable error messages. Empty vec means the value is
/// valid.
pub fn validate_bidder_params(validator: &JSONSchema, params: &Value) -> Vec<String> {
    match validator.validate(params) {
        Ok(()) => Vec::new(),
        Err(errors) => errors.map(|e| e.to_string()).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    const CANNED_SCHEMA: &str = r#"{
        "$schema": "http://json-schema.org/draft-07/schema#",
        "title": "Canned Bidder Params",
        "type": "object",
        "properties": {
            "placement_id": { "type": "integer" },
            "site": { "type": "string" }
        },
        "required": ["placement_id"]
    }"#;

    fn write_schema_dir() -> tempfile::TempDir {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("canned.json"), CANNED_SCHEMA).unwrap();
        // Non-json file should be ignored.
        fs::write(dir.path().join("README.md"), "ignore").unwrap();
        dir
    }

    #[test]
    fn test_load_bidder_params_from_dir_compiles() {
        let dir = write_schema_dir();
        let map = load_bidder_params_from_dir(dir.path()).unwrap();
        assert_eq!(map.len(), 1);
        assert!(map.contains_key("canned"));
    }

    #[test]
    fn test_validate_bidder_params_conforming_doc() {
        let dir = write_schema_dir();
        let map = load_bidder_params_from_dir(dir.path()).unwrap();
        let validator = &map["canned"];

        let good = json!({ "placement_id": 42, "site": "example.com" });
        let errors = validate_bidder_params(validator, &good);
        assert!(
            errors.is_empty(),
            "expected no errors, got: {:?}",
            errors
        );
    }

    #[test]
    fn test_validate_bidder_params_non_conforming_doc() {
        let dir = write_schema_dir();
        let map = load_bidder_params_from_dir(dir.path()).unwrap();
        let validator = &map["canned"];

        // Missing required field and wrong type.
        let bad = json!({ "site": 99 });
        let errors = validate_bidder_params(validator, &bad);
        assert!(
            !errors.is_empty(),
            "expected validation errors for non-conforming doc"
        );
    }

    #[test]
    fn test_load_bidder_params_missing_dir() {
        let err = load_bidder_params_from_dir(Path::new("/definitely/not/here/zzz"));
        assert!(err.is_err());
    }
}
