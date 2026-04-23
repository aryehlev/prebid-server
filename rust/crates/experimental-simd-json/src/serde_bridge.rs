//! Conversions between [`simd_json::OwnedValue`] and [`serde_json::Value`].
//!
//! These helpers let the experimental parser feed its output into code that
//! still operates on `serde_json`, and vice versa. Conversions are recursive
//! and lossless for the JSON data model.

use serde_json::{Map as JsonMap, Number as JsonNumber, Value as JsonValue};
use simd_json::{OwnedValue, StaticNode};

/// Convert an owned simd-json value into a `serde_json::Value`.
pub fn to_serde_json(owned: OwnedValue) -> JsonValue {
    match owned {
        OwnedValue::Static(StaticNode::Null) => JsonValue::Null,
        OwnedValue::Static(StaticNode::Bool(b)) => JsonValue::Bool(b),
        OwnedValue::Static(StaticNode::I64(i)) => JsonValue::Number(i.into()),
        OwnedValue::Static(StaticNode::U64(u)) => JsonValue::Number(u.into()),
        OwnedValue::Static(StaticNode::F64(f)) => JsonNumber::from_f64(f)
            .map(JsonValue::Number)
            .unwrap_or(JsonValue::Null),
        OwnedValue::String(s) => JsonValue::String(s),
        OwnedValue::Array(arr) => {
            JsonValue::Array(arr.into_iter().map(to_serde_json).collect())
        }
        OwnedValue::Object(obj) => {
            let mut map = JsonMap::with_capacity(obj.len());
            for (k, v) in *obj {
                map.insert(k, to_serde_json(v));
            }
            JsonValue::Object(map)
        }
    }
}

/// Convert a `serde_json::Value` into an owned simd-json value.
pub fn from_serde_json(v: &JsonValue) -> OwnedValue {
    match v {
        JsonValue::Null => OwnedValue::Static(StaticNode::Null),
        JsonValue::Bool(b) => OwnedValue::Static(StaticNode::Bool(*b)),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                OwnedValue::Static(StaticNode::I64(i))
            } else if let Some(u) = n.as_u64() {
                OwnedValue::Static(StaticNode::U64(u))
            } else if let Some(f) = n.as_f64() {
                OwnedValue::Static(StaticNode::F64(f))
            } else {
                OwnedValue::Static(StaticNode::Null)
            }
        }
        JsonValue::String(s) => OwnedValue::String(s.clone()),
        JsonValue::Array(arr) => {
            OwnedValue::Array(Box::new(arr.iter().map(from_serde_json).collect()))
        }
        JsonValue::Object(obj) => {
            let mut map = simd_json::owned::Object::with_capacity(obj.len());
            for (k, val) in obj {
                map.insert(k.clone(), from_serde_json(val));
            }
            OwnedValue::Object(Box::new(map))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn round_trips_nested_object() {
        let original = json!({
            "id": "req-1",
            "tmax": 250,
            "flag": true,
            "items": [1, 2, {"nested": "x", "f": 1.5}],
            "null_field": null,
        });
        let owned = from_serde_json(&original);
        let back = to_serde_json(owned);
        assert_eq!(original, back);
    }

    #[test]
    fn round_trips_primitives() {
        for v in [
            json!(null),
            json!(true),
            json!(false),
            json!(42),
            json!(-7),
            json!("hello"),
            json!([]),
            json!({}),
        ] {
            let owned = from_serde_json(&v);
            assert_eq!(to_serde_json(owned), v);
        }
    }
}
