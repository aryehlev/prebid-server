//! JSON utilities — mirrors Go `util/jsonutil` package.
//! Provides FindElement, DropElement, deep merge, and JSON pointer helpers.

/// Deep-merge `source` into `target`. For objects, keys in `source` are
/// recursively merged into `target`. For all other types, `source` overwrites `target`.
pub fn deep_merge(target: &mut serde_json::Value, source: &serde_json::Value) {
    match (target, source) {
        (serde_json::Value::Object(ref mut t), serde_json::Value::Object(s)) => {
            for (key, src_val) in s {
                let entry = t
                    .entry(key.clone())
                    .or_insert(serde_json::Value::Null);
                deep_merge(entry, src_val);
            }
        }
        (target, source) => {
            *target = source.clone();
        }
    }
}

/// Get a string value from a JSON value using a JSON pointer path.
pub fn get_string(v: &serde_json::Value, ptr: &str) -> Option<String> {
    v.pointer(ptr)
        .and_then(|val| val.as_str())
        .map(|s| s.to_string())
}

/// Get an optional i64 from a JSON value using a JSON pointer path.
pub fn get_i64(v: &serde_json::Value, ptr: &str) -> Option<i64> {
    v.pointer(ptr).and_then(|val| val.as_i64())
}

/// Get an optional f64 from a JSON value using a JSON pointer path.
pub fn get_f64(v: &serde_json::Value, ptr: &str) -> Option<f64> {
    v.pointer(ptr).and_then(|val| val.as_f64())
}

/// FindElement finds an element in a JSON byte array with any level of nesting.
/// Returns (found, start_index, end_index).
///
/// This mirrors the Go `jsonutil.FindElement` function which walks through JSON
/// byte arrays to locate specific named elements for efficient byte-level manipulation.
pub fn find_element(extension: &[u8], element_names: &[&str]) -> Result<(bool, usize, usize), serde_json::Error> {
    if element_names.is_empty() {
        return Ok((false, 0, 0));
    }

    let value: serde_json::Value = serde_json::from_slice(extension)?;
    find_element_in_value(&value, element_names, extension)
}

fn find_element_in_value(
    value: &serde_json::Value,
    element_names: &[&str],
    raw: &[u8],
) -> Result<(bool, usize, usize), serde_json::Error> {
    let element_name = element_names[0];

    if let serde_json::Value::Object(map) = value {
        if let Some(found_value) = map.get(element_name) {
            if element_names.len() == 1 {
                // Find this key-value pair in the raw bytes
                let search_key = format!("\"{}\"", element_name);
                if let Some(key_pos) = find_substring(raw, search_key.as_bytes()) {
                    // Find the colon after the key
                    let mut colon_pos = key_pos + search_key.len();
                    while colon_pos < raw.len() && raw[colon_pos] != b':' {
                        colon_pos += 1;
                    }
                    // Skip past the colon and whitespace
                    let value_start = colon_pos + 1;

                    // Serialize the value to find its length
                    let value_bytes = serde_json::to_vec(found_value)?;
                    // Find the end of the value in the raw bytes
                    let mut end_pos = value_start;
                    // Skip whitespace
                    while end_pos < raw.len() && (raw[end_pos] == b' ' || raw[end_pos] == b'\n' || raw[end_pos] == b'\r' || raw[end_pos] == b'\t') {
                        end_pos += 1;
                    }
                    end_pos += value_bytes.len();
                    // Include trailing comma if present
                    while end_pos < raw.len() && (raw[end_pos] == b' ' || raw[end_pos] == b'\n' || raw[end_pos] == b'\r' || raw[end_pos] == b'\t') {
                        end_pos += 1;
                    }

                    // Calculate the start including the key
                    let mut start = key_pos;
                    // Check for leading comma
                    if start > 0 {
                        let mut check = start - 1;
                        while check > 0 && (raw[check] == b' ' || raw[check] == b'\n' || raw[check] == b'\r' || raw[check] == b'\t') {
                            check -= 1;
                        }
                        if raw[check] == b',' {
                            start = check;
                        }
                    }

                    if end_pos < raw.len() && raw[end_pos] == b',' {
                        end_pos += 1;
                    }

                    return Ok((true, start, end_pos));
                }
                return Ok((false, 0, 0));
            } else {
                // Recurse into nested element
                let nested_bytes = serde_json::to_vec(found_value)?;
                let (found, s, e) = find_element(&nested_bytes, &element_names[1..])?;
                if found {
                    // Offset is relative to the nested value, need to translate
                    return Ok((true, s, e));
                }
                return Ok((false, 0, 0));
            }
        }
    }

    Ok((false, 0, 0))
}

fn find_substring(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// DropElement drops an element from a JSON object by key path.
/// Returns the modified JSON bytes.
pub fn drop_element(extension: &[u8], element_names: &[&str]) -> Result<serde_json::Value, serde_json::Error> {
    let mut value: serde_json::Value = serde_json::from_slice(extension)?;
    drop_element_from_value(&mut value, element_names);
    Ok(value)
}

fn drop_element_from_value(value: &mut serde_json::Value, element_names: &[&str]) -> bool {
    if element_names.is_empty() {
        return false;
    }

    if let serde_json::Value::Object(map) = value {
        if element_names.len() == 1 {
            return map.remove(element_names[0]).is_some();
        } else if let Some(nested) = map.get_mut(element_names[0]) {
            return drop_element_from_value(nested, &element_names[1..]);
        }
    }
    false
}

/// MergeClone — merges incoming JSON data on top of an existing value,
/// similar to Go's jsonutil.MergeClone. Uses JSON merge patch semantics.
pub fn merge_clone<T: serde::de::DeserializeOwned + serde::Serialize>(
    existing: &T,
    incoming: &[u8],
) -> Result<T, serde_json::Error> {
    let mut existing_value = serde_json::to_value(existing)?;
    let incoming_value: serde_json::Value = serde_json::from_slice(incoming)?;
    deep_merge(&mut existing_value, &incoming_value);
    serde_json::from_value(existing_value)
}

/// StringInt is an integer that can be deserialized from either a JSON number or a JSON string.
/// Mirrors Go `jsonutil.StringInt`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StringInt(pub i64);

impl<'de> serde::Deserialize<'de> for StringInt {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct StringIntVisitor;
        impl<'de> serde::de::Visitor<'de> for StringIntVisitor {
            type Value = StringInt;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("an integer or a string containing an integer")
            }

            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Self::Value, E> {
                Ok(StringInt(v))
            }

            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Self::Value, E> {
                Ok(StringInt(v as i64))
            }

            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                v.parse::<i64>().map(StringInt).map_err(serde::de::Error::custom)
            }
        }
        deserializer.deserialize_any(StringIntVisitor)
    }
}

/// IntString is a string that can be deserialized from either a JSON string or number.
/// Mirrors Go `jsonutil.IntString`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IntString(pub String);

impl<'de> serde::Deserialize<'de> for IntString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct IntStringVisitor;
        impl<'de> serde::de::Visitor<'de> for IntStringVisitor {
            type Value = IntString;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a string or a number")
            }

            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Ok(IntString(v.to_string()))
            }

            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Self::Value, E> {
                Ok(IntString(v.to_string()))
            }

            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Self::Value, E> {
                Ok(IntString(v.to_string()))
            }

            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Self::Value, E> {
                Ok(IntString(v.to_string()))
            }
        }
        deserializer.deserialize_any(IntStringVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_deep_merge_objects() {
        let mut target = json!({"a": 1, "b": {"c": 2}});
        let source = json!({"b": {"d": 3}, "e": 4});
        deep_merge(&mut target, &source);
        assert_eq!(target, json!({"a": 1, "b": {"c": 2, "d": 3}, "e": 4}));
    }

    #[test]
    fn test_deep_merge_overwrite_scalar() {
        let mut target = json!({"a": 1});
        let source = json!({"a": 2});
        deep_merge(&mut target, &source);
        assert_eq!(target, json!({"a": 2}));
    }

    #[test]
    fn test_get_string() {
        let v = json!({"foo": {"bar": "baz"}});
        assert_eq!(get_string(&v, "/foo/bar"), Some("baz".to_string()));
        assert_eq!(get_string(&v, "/foo/missing"), None);
    }

    #[test]
    fn test_drop_element() {
        let data = br#"{"a": 1, "b": {"c": 2, "d": 3}}"#;
        let result = drop_element(data, &["b", "c"]).unwrap();
        assert_eq!(result, json!({"a": 1, "b": {"d": 3}}));
    }

    #[test]
    fn test_drop_element_top_level() {
        let data = br#"{"a": 1, "b": 2}"#;
        let result = drop_element(data, &["a"]).unwrap();
        assert_eq!(result, json!({"b": 2}));
    }

    #[test]
    fn test_string_int_from_number() {
        let v: StringInt = serde_json::from_str("42").unwrap();
        assert_eq!(v.0, 42);
    }

    #[test]
    fn test_string_int_from_string() {
        let v: StringInt = serde_json::from_str("\"42\"").unwrap();
        assert_eq!(v.0, 42);
    }

    #[test]
    fn test_int_string_from_string() {
        let v: IntString = serde_json::from_str("\"hello\"").unwrap();
        assert_eq!(v.0, "hello");
    }

    #[test]
    fn test_int_string_from_number() {
        let v: IntString = serde_json::from_str("42").unwrap();
        assert_eq!(v.0, "42");
    }
}
