use std::collections::HashMap;
use std::net::{Ipv4Addr, Ipv6Addr};

use uuid::Uuid;

/// Generate a new UUID v4 string
pub fn new_uuid() -> String {
    Uuid::new_v4().to_string()
}

/// Truncate a float to the specified number of decimal places
pub fn truncate_decimal_places(val: f64, places: u32) -> f64 {
    let factor = 10_f64.powi(places as i32);
    (val * factor).trunc() / factor
}

/// Get a string value from a JSON value using a JSON pointer path
pub fn get_string(v: &serde_json::Value, ptr: &str) -> Option<String> {
    v.pointer(ptr)
        .and_then(|val| val.as_str())
        .map(|s| s.to_string())
}

/// Get an optional i64 from a JSON value using a JSON pointer path
pub fn get_i64(v: &serde_json::Value, ptr: &str) -> Option<i64> {
    v.pointer(ptr).and_then(|val| val.as_i64())
}

/// Get an optional f64 from a JSON value using a JSON pointer path
pub fn get_f64(v: &serde_json::Value, ptr: &str) -> Option<f64> {
    v.pointer(ptr).and_then(|val| val.as_f64())
}

/// Sanitize an IP address: strip IPv6 prefix and port information
pub fn sanitize_ip(ip: &str) -> String {
    let ip = ip.trim();

    // Strip IPv6-mapped IPv4 prefix
    let ip = if let Some(stripped) = ip.strip_prefix("::ffff:") {
        stripped
    } else {
        ip
    };

    // Handle IPv6 addresses with port: [::1]:port
    if ip.starts_with('[') {
        if let Some(end) = ip.find(']') {
            return ip[1..end].to_string();
        }
    }

    // Handle IPv4 with port: 1.2.3.4:port
    // Only strip port if there's exactly one colon (IPv4 with port, not IPv6)
    let colon_count = ip.chars().filter(|&c| c == ':').count();
    if colon_count == 1 {
        if let Some(pos) = ip.rfind(':') {
            return ip[..pos].to_string();
        }
    }

    ip.to_string()
}

// ---------------------------------------------------------------------------
// JSON utilities
// ---------------------------------------------------------------------------

/// Deep-merge `source` into `target`. For objects, keys in `source` are
/// recursively merged into `target`. For all other types, `source` overwrites
/// `target`.
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

// ---------------------------------------------------------------------------
// Slice / collection utilities
// ---------------------------------------------------------------------------

/// Return a new Vec with duplicate strings removed, preserving first-occurrence order.
pub fn unique_strings(items: &[&str]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for &item in items {
        if seen.insert(item) {
            result.push(item.to_string());
        }
    }
    result
}

/// Check whether a string slice contains the given value.
pub fn contains_string(items: &[&str], target: &str) -> bool {
    items.iter().any(|&s| s == target)
}

/// Shuffle a Vec in-place using Fisher-Yates (via `rand`).
pub fn random_shuffle<T>(items: &mut Vec<T>) {
    use rand::seq::SliceRandom;
    let mut rng = rand::thread_rng();
    items.shuffle(&mut rng);
}

// ---------------------------------------------------------------------------
// IP privacy utilities
// ---------------------------------------------------------------------------

/// Mask an IPv4 address, keeping only the first `bits_to_keep` bits and
/// zeroing the rest. Useful for privacy/GDPR compliance.
///
/// Example: mask_ipv4("192.168.1.100", 24) => "192.168.1.0"
pub fn mask_ipv4(ip: &str, bits_to_keep: u8) -> Option<String> {
    let addr: Ipv4Addr = ip.parse().ok()?;
    if bits_to_keep >= 32 {
        return Some(addr.to_string());
    }
    if bits_to_keep == 0 {
        return Some("0.0.0.0".to_string());
    }
    let raw: u32 = u32::from(addr);
    let mask: u32 = !0u32 << (32 - bits_to_keep);
    let masked = Ipv4Addr::from(raw & mask);
    Some(masked.to_string())
}

/// Mask an IPv6 address, keeping only the first `bits_to_keep` bits and
/// zeroing the rest.
///
/// Example: mask_ipv6("2001:db8::1", 32) => "2001:db8::"
pub fn mask_ipv6(ip: &str, bits_to_keep: u8) -> Option<String> {
    let addr: Ipv6Addr = ip.parse().ok()?;
    if bits_to_keep >= 128 {
        return Some(addr.to_string());
    }
    if bits_to_keep == 0 {
        return Some("::".to_string());
    }
    let raw: u128 = u128::from(addr);
    let mask: u128 = !0u128 << (128 - bits_to_keep);
    let masked = Ipv6Addr::from(raw & mask);
    Some(masked.to_string())
}

// ---------------------------------------------------------------------------
// HTTP utilities
// ---------------------------------------------------------------------------

/// Parse a query string (e.g. "foo=bar&baz=qux") into a HashMap.
/// Handles URL-encoded values. Duplicate keys keep the last value.
pub fn parse_query_string_params(query: &str) -> HashMap<String, String> {
    let query = query.strip_prefix('?').unwrap_or(query);
    let parsed = url::form_urlencoded::parse(query.as_bytes());
    parsed
        .into_iter()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect()
}

/// Encode a map of parameters into a URL-encoded query string.
/// Keys are sorted for deterministic output.
pub fn encode_url_params(params: &HashMap<String, String>) -> String {
    let mut pairs: Vec<(&String, &String)> = params.iter().collect();
    pairs.sort_by_key(|(k, _)| k.as_str());
    let encoded: String = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(pairs)
        .finish();
    encoded
}

// ---------------------------------------------------------------------------
// String utilities
// ---------------------------------------------------------------------------

/// Truncate a string to at most `max_len` bytes, ensuring we don't split a
/// UTF-8 character. The result will be <= max_len bytes.
pub fn truncate_utf8(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        return s;
    }
    // Find the largest char boundary <= max_len
    let mut end = max_len;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Check whether a string is a syntactically valid URL (has scheme + host).
pub fn is_valid_url(s: &str) -> bool {
    match url::Url::parse(s) {
        Ok(u) => u.has_host(),
        Err(_) => false,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_new_uuid() {
        let id = new_uuid();
        assert_eq!(id.len(), 36);
    }

    #[test]
    fn test_truncate_decimal_places() {
        assert_eq!(truncate_decimal_places(3.14159, 2), 3.14);
        assert_eq!(truncate_decimal_places(1.999, 1), 1.9);
    }

    #[test]
    fn test_get_string() {
        let v = json!({"foo": {"bar": "baz"}});
        assert_eq!(get_string(&v, "/foo/bar"), Some("baz".to_string()));
        assert_eq!(get_string(&v, "/foo/missing"), None);
    }

    #[test]
    fn test_sanitize_ip() {
        assert_eq!(sanitize_ip("1.2.3.4"), "1.2.3.4");
        assert_eq!(sanitize_ip("1.2.3.4:80"), "1.2.3.4");
        assert_eq!(sanitize_ip("::ffff:1.2.3.4"), "1.2.3.4");
        assert_eq!(sanitize_ip("[::1]:8080"), "::1");
        assert_eq!(sanitize_ip("2001:db8::1"), "2001:db8::1");
    }

    // -- JSON deep_merge tests --

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
    fn test_deep_merge_nested_overwrite() {
        let mut target = json!({"a": {"b": 1}});
        let source = json!({"a": {"b": 2, "c": 3}});
        deep_merge(&mut target, &source);
        assert_eq!(target, json!({"a": {"b": 2, "c": 3}}));
    }

    #[test]
    fn test_deep_merge_source_replaces_non_object() {
        let mut target = json!({"a": "string"});
        let source = json!({"a": {"nested": true}});
        deep_merge(&mut target, &source);
        assert_eq!(target, json!({"a": {"nested": true}}));
    }

    #[test]
    fn test_deep_merge_array_replaced() {
        let mut target = json!({"a": [1, 2]});
        let source = json!({"a": [3, 4, 5]});
        deep_merge(&mut target, &source);
        // Arrays are replaced, not merged element-wise
        assert_eq!(target, json!({"a": [3, 4, 5]}));
    }

    // -- Slice utility tests --

    #[test]
    fn test_unique_strings() {
        let result = unique_strings(&["a", "b", "a", "c", "b"]);
        assert_eq!(result, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_unique_strings_empty() {
        let result = unique_strings(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_contains_string() {
        assert!(contains_string(&["a", "b", "c"], "b"));
        assert!(!contains_string(&["a", "b", "c"], "d"));
        assert!(!contains_string(&[], "a"));
    }

    #[test]
    fn test_random_shuffle() {
        let mut items = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        let original = items.clone();
        random_shuffle(&mut items);
        // Same elements
        let mut sorted = items.clone();
        sorted.sort();
        let mut orig_sorted = original.clone();
        orig_sorted.sort();
        assert_eq!(sorted, orig_sorted);
        // With 10 elements, the chance of shuffle producing the same order is 1/10! ~ 0
        // We don't assert inequality since it's theoretically possible, just check length.
        assert_eq!(items.len(), 10);
    }

    // -- IP masking tests --

    #[test]
    fn test_mask_ipv4_24() {
        assert_eq!(mask_ipv4("192.168.1.100", 24), Some("192.168.1.0".to_string()));
    }

    #[test]
    fn test_mask_ipv4_16() {
        assert_eq!(mask_ipv4("192.168.1.100", 16), Some("192.168.0.0".to_string()));
    }

    #[test]
    fn test_mask_ipv4_full() {
        assert_eq!(
            mask_ipv4("192.168.1.100", 32),
            Some("192.168.1.100".to_string())
        );
    }

    #[test]
    fn test_mask_ipv4_zero() {
        assert_eq!(mask_ipv4("192.168.1.100", 0), Some("0.0.0.0".to_string()));
    }

    #[test]
    fn test_mask_ipv4_invalid() {
        assert_eq!(mask_ipv4("not_an_ip", 24), None);
    }

    #[test]
    fn test_mask_ipv6_32() {
        assert_eq!(
            mask_ipv6("2001:db8::1", 32),
            Some("2001:db8::".to_string())
        );
    }

    #[test]
    fn test_mask_ipv6_64() {
        assert_eq!(
            mask_ipv6("2001:db8:abcd:1234::1", 64),
            Some("2001:db8:abcd:1234::".to_string())
        );
    }

    #[test]
    fn test_mask_ipv6_full() {
        assert_eq!(
            mask_ipv6("2001:db8::1", 128),
            Some("2001:db8::1".to_string())
        );
    }

    #[test]
    fn test_mask_ipv6_zero() {
        assert_eq!(mask_ipv6("2001:db8::1", 0), Some("::".to_string()));
    }

    #[test]
    fn test_mask_ipv6_invalid() {
        assert_eq!(mask_ipv6("not_an_ip", 64), None);
    }

    // -- HTTP utility tests --

    #[test]
    fn test_parse_query_string_basic() {
        let params = parse_query_string_params("foo=bar&baz=qux");
        assert_eq!(params.get("foo"), Some(&"bar".to_string()));
        assert_eq!(params.get("baz"), Some(&"qux".to_string()));
    }

    #[test]
    fn test_parse_query_string_with_question_mark() {
        let params = parse_query_string_params("?foo=bar&baz=qux");
        assert_eq!(params.get("foo"), Some(&"bar".to_string()));
    }

    #[test]
    fn test_parse_query_string_encoded() {
        let params = parse_query_string_params("name=hello+world&key=a%26b");
        assert_eq!(params.get("name"), Some(&"hello world".to_string()));
        assert_eq!(params.get("key"), Some(&"a&b".to_string()));
    }

    #[test]
    fn test_parse_query_string_empty() {
        let params = parse_query_string_params("");
        assert!(params.is_empty());
    }

    #[test]
    fn test_encode_url_params() {
        let mut params = HashMap::new();
        params.insert("foo".to_string(), "bar".to_string());
        params.insert("baz".to_string(), "hello world".to_string());
        let encoded = encode_url_params(&params);
        // Sorted by key, so baz comes before foo
        assert_eq!(encoded, "baz=hello+world&foo=bar");
    }

    #[test]
    fn test_encode_url_params_empty() {
        let params = HashMap::new();
        let encoded = encode_url_params(&params);
        assert_eq!(encoded, "");
    }

    // -- String utility tests --

    #[test]
    fn test_truncate_utf8_ascii() {
        assert_eq!(truncate_utf8("hello world", 5), "hello");
    }

    #[test]
    fn test_truncate_utf8_no_truncation() {
        assert_eq!(truncate_utf8("hi", 10), "hi");
    }

    #[test]
    fn test_truncate_utf8_multibyte() {
        // "é" is 2 bytes in UTF-8
        let s = "café";
        // "caf" = 3 bytes, "é" = 2 bytes, total = 5 bytes
        assert_eq!(truncate_utf8(s, 4), "caf");
        assert_eq!(truncate_utf8(s, 5), "café");
    }

    #[test]
    fn test_truncate_utf8_emoji() {
        // "😀" is 4 bytes
        let s = "a😀b";
        assert_eq!(truncate_utf8(s, 1), "a");
        assert_eq!(truncate_utf8(s, 2), "a"); // can't split emoji
        assert_eq!(truncate_utf8(s, 5), "a😀");
        assert_eq!(truncate_utf8(s, 6), "a😀b");
    }

    #[test]
    fn test_truncate_utf8_zero() {
        assert_eq!(truncate_utf8("hello", 0), "");
    }

    #[test]
    fn test_is_valid_url() {
        assert!(is_valid_url("https://example.com"));
        assert!(is_valid_url("http://example.com/path?q=1"));
        assert!(is_valid_url("https://sub.domain.com:8080/"));
        assert!(!is_valid_url("not a url"));
        assert!(!is_valid_url(""));
        assert!(!is_valid_url("ftp://"));
        assert!(!is_valid_url("://missing-scheme.com"));
    }
}
