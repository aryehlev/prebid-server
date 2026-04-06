//! Map utilities — mirrors Go `util/maputil` package.

use serde_json::Value;

/// Read an embedded map from a JSON Value object.
pub fn read_embedded_map<'a>(m: &'a Value, k: &str) -> Option<&'a serde_json::Map<String, Value>> {
    m.get(k).and_then(|v| v.as_object())
}

/// Read an embedded array from a JSON Value object.
pub fn read_embedded_slice<'a>(m: &'a Value, k: &str) -> Option<&'a Vec<Value>> {
    m.get(k).and_then(|v| v.as_array())
}

/// Read an embedded string from a JSON Value object.
pub fn read_embedded_string<'a>(m: &'a Value, k: &str) -> Option<&'a str> {
    m.get(k).and_then(|v| v.as_str())
}

/// Check if a nested element exists at the given key path.
pub fn has_element(m: &Value, keys: &[&str]) -> bool {
    let mut current = m;
    for (i, key) in keys.iter().enumerate() {
        match current.get(key) {
            Some(v) => {
                if i == keys.len() - 1 {
                    return true;
                }
                current = v;
            }
            None => return false,
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_read_embedded_string() {
        let v = json!({"foo": "bar"});
        assert_eq!(read_embedded_string(&v, "foo"), Some("bar"));
        assert_eq!(read_embedded_string(&v, "missing"), None);
    }

    #[test]
    fn test_has_element() {
        let v = json!({"a": {"b": {"c": 1}}});
        assert!(has_element(&v, &["a", "b", "c"]));
        assert!(has_element(&v, &["a", "b"]));
        assert!(!has_element(&v, &["a", "b", "d"]));
        assert!(!has_element(&v, &["x"]));
    }
}
