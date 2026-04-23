//! JSON merge/clone utility.
//!
//! Mirrors Go `util/jsonutil/merge.go`.
//!
//! Provides `merge_clone` which unmarshals JSON into a struct while cloning
//! all nested references (pointers, slices, maps) and applying merge-patch
//! semantics for `serde_json::Value` fields.

use serde_json::Value;

/// Deep-merge `overlay` into `base` using JSON Merge Patch semantics (RFC 7386).
///
/// - For objects: recursively merge keys. Overlay keys with `null` values
///   remove the key from base.
/// - For non-objects: overlay replaces base entirely.
/// - If overlay is null, returns null (deletes the value).
///
/// Mirrors Go `jsonpatch.MergePatch`.
pub fn merge_patch(base: &Value, patch: &Value) -> Value {
    if !patch.is_object() {
        return patch.clone();
    }

    let mut result = if base.is_object() {
        base.clone()
    } else {
        Value::Object(serde_json::Map::new())
    };

    if let (Some(result_obj), Some(patch_obj)) = (result.as_object_mut(), patch.as_object()) {
        for (key, patch_val) in patch_obj {
            if patch_val.is_null() {
                result_obj.remove(key);
            } else if let Some(base_val) = result_obj.get(key).cloned() {
                let merged = merge_patch(&base_val, patch_val);
                result_obj.insert(key.clone(), merged);
            } else {
                result_obj.insert(key.clone(), patch_val.clone());
            }
        }
    }

    result
}

/// Merge two JSON values, with `existing` taking precedence for conflicting keys.
/// New keys from `incoming` are added.
///
/// This is a convenience wrapper around `merge_patch` with swapped priority.
pub fn merge_clone_json(existing: &Value, incoming: &Value) -> Value {
    if existing.is_null() || (existing.is_object() && existing.as_object().unwrap().is_empty()) {
        return incoming.clone();
    }
    if incoming.is_null() || (incoming.is_object() && incoming.as_object().unwrap().is_empty()) {
        return existing.clone();
    }

    // Use merge_patch but with incoming as base, existing as patch
    // so existing keys take precedence
    merge_patch(incoming, existing)
}

/// Apply a JSON overlay to a struct by serializing the struct to JSON,
/// merging, and deserializing back.
///
/// Mirrors Go `MergeClone(v any, data json.RawMessage)`.
pub fn merge_clone<T>(target: &T, overlay: &[u8]) -> Result<T, String>
where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    let base_json = serde_json::to_value(target)
        .map_err(|e| format!("failed to serialize base: {}", e))?;

    let overlay_json: Value = serde_json::from_slice(overlay)
        .map_err(|e| format!("failed to parse overlay JSON: {}", e))?;

    let merged = merge_patch(&base_json, &overlay_json);

    serde_json::from_value(merged).map_err(|e| format!("failed to unmarshal merged result: {}", e))
}

/// Apply a stored request JSON to a bid request JSON.
///
/// The stored request provides defaults; any fields already present in the
/// incoming request take precedence.
///
/// Mirrors the stored request patching flow in Go's `exchange/utils.go`.
pub fn apply_stored_request(request_json: &[u8], stored_json: &[u8]) -> Result<Vec<u8>, String> {
    let req: Value =
        serde_json::from_slice(request_json).map_err(|e| format!("bad request JSON: {}", e))?;
    let stored: Value =
        serde_json::from_slice(stored_json).map_err(|e| format!("bad stored JSON: {}", e))?;

    // Stored request is the base, incoming request fields override
    let merged = merge_patch(&stored, &req);

    serde_json::to_vec(&merged).map_err(|e| format!("failed to marshal merged request: {}", e))
}

/// Apply a stored impression JSON to an impression JSON.
pub fn apply_stored_imp(imp_json: &[u8], stored_json: &[u8]) -> Result<Vec<u8>, String> {
    apply_stored_request(imp_json, stored_json)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_merge_patch_simple_override() {
        let base = json!({"a": 1, "b": 2});
        let patch = json!({"b": 3, "c": 4});
        let result = merge_patch(&base, &patch);
        assert_eq!(result, json!({"a": 1, "b": 3, "c": 4}));
    }

    #[test]
    fn test_merge_patch_null_removes_key() {
        let base = json!({"a": 1, "b": 2});
        let patch = json!({"b": null});
        let result = merge_patch(&base, &patch);
        assert_eq!(result, json!({"a": 1}));
    }

    #[test]
    fn test_merge_patch_nested_objects() {
        let base = json!({"a": {"x": 1, "y": 2}, "b": 3});
        let patch = json!({"a": {"y": 9, "z": 10}});
        let result = merge_patch(&base, &patch);
        assert_eq!(result, json!({"a": {"x": 1, "y": 9, "z": 10}, "b": 3}));
    }

    #[test]
    fn test_merge_patch_array_replaces() {
        let base = json!({"a": [1, 2, 3]});
        let patch = json!({"a": [4, 5]});
        let result = merge_patch(&base, &patch);
        assert_eq!(result, json!({"a": [4, 5]}));
    }

    #[test]
    fn test_merge_patch_non_object_patch() {
        let base = json!({"a": 1});
        let patch = json!("hello");
        let result = merge_patch(&base, &patch);
        assert_eq!(result, json!("hello"));
    }

    #[test]
    fn test_merge_clone_json_existing_wins() {
        let existing = json!({"a": 1, "b": 2});
        let incoming = json!({"b": 99, "c": 3});
        let result = merge_clone_json(&existing, &incoming);
        assert_eq!(result["a"], json!(1));
        assert_eq!(result["b"], json!(2)); // existing wins
        assert_eq!(result["c"], json!(3)); // new key from incoming
    }

    #[test]
    fn test_merge_clone_json_empty_existing() {
        let existing = json!({});
        let incoming = json!({"a": 1});
        let result = merge_clone_json(&existing, &incoming);
        assert_eq!(result, json!({"a": 1}));
    }

    #[test]
    fn test_merge_clone_json_null_incoming() {
        let existing = json!({"a": 1});
        let incoming = Value::Null;
        let result = merge_clone_json(&existing, &incoming);
        assert_eq!(result, json!({"a": 1}));
    }

    #[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
    struct TestConfig {
        #[serde(default)]
        name: String,
        #[serde(default)]
        count: u32,
        #[serde(default)]
        enabled: bool,
    }

    #[test]
    fn test_merge_clone_struct() {
        let base = TestConfig {
            name: "original".to_string(),
            count: 10,
            enabled: true,
        };
        let overlay = br#"{"count": 20, "enabled": false}"#;
        let result: TestConfig = merge_clone(&base, overlay).unwrap();
        assert_eq!(result.name, "original"); // unchanged
        assert_eq!(result.count, 20); // overridden
        assert!(!result.enabled); // overridden
    }

    #[test]
    fn test_merge_clone_struct_empty_overlay() {
        let base = TestConfig {
            name: "test".to_string(),
            count: 5,
            enabled: true,
        };
        let result: TestConfig = merge_clone(&base, b"{}").unwrap();
        assert_eq!(result, base);
    }

    #[test]
    fn test_apply_stored_request() {
        let request = br#"{"id": "req-1", "imp": [{"id": "imp-1"}]}"#;
        let stored = br#"{"tmax": 500, "ext": {"prebid": {}}}"#;
        let result = apply_stored_request(request, stored).unwrap();
        let val: Value = serde_json::from_slice(&result).unwrap();
        assert_eq!(val["id"], json!("req-1")); // from request
        assert_eq!(val["tmax"], json!(500)); // from stored
        assert!(val["imp"].is_array()); // from request
    }

    #[test]
    fn test_apply_stored_request_override() {
        let request = br#"{"tmax": 200}"#;
        let stored = br#"{"tmax": 500, "at": 1}"#;
        let result = apply_stored_request(request, stored).unwrap();
        let val: Value = serde_json::from_slice(&result).unwrap();
        assert_eq!(val["tmax"], json!(200)); // request overrides
        assert_eq!(val["at"], json!(1)); // from stored
    }

    #[test]
    fn test_apply_stored_request_bad_json() {
        let result = apply_stored_request(b"not json", b"{}");
        assert!(result.is_err());
    }
}
