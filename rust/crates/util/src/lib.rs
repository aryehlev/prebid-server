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

#[cfg(test)]
mod tests {
    use super::*;

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
        let v = serde_json::json!({"foo": {"bar": "baz"}});
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
}
