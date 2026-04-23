//! HTTP utilities — mirrors Go `util/httputil` package.
//! Provides IP extraction from HTTP headers and URL parameter handling.

use std::collections::HashMap;
use std::net::IpAddr;

use crate::iputil::{self, IpValidator, IpVersion};

/// Content encoding type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentEncoding {
    Gzip,
    Other(String),
}

impl ContentEncoding {
    pub fn normalize(&self) -> ContentEncoding {
        match self {
            ContentEncoding::Gzip => ContentEncoding::Gzip,
            ContentEncoding::Other(s) => {
                let lower = s.to_lowercase();
                if lower == "gzip" {
                    ContentEncoding::Gzip
                } else {
                    ContentEncoding::Other(lower)
                }
            }
        }
    }
}

/// Represents an HTTP request's header map and remote address, for IP extraction.
/// This is a simplified representation since Rust doesn't use Go's http.Request.
pub struct RequestHeaders {
    pub headers: HashMap<String, Vec<String>>,
    pub remote_addr: String,
}

/// Find the first valid IP address from HTTP request headers.
/// Checks True-Client-IP, X-Forwarded-For, X-Real-IP, and RemoteAddr in that order.
pub fn find_ip(req: &RequestHeaders, validator: &dyn IpValidator) -> (Option<IpAddr>, IpVersion) {
    // Try True-Client-IP
    if let Some(values) = req.headers.get("true-client-ip") {
        if let Some(value) = values.first() {
            let trimmed = value.trim();
            let (ip, ver) = iputil::parse_ip(trimmed);
            if let Some(ref ip) = ip {
                if validator.is_valid(ip, ver) {
                    return (Some(*ip), ver);
                }
            }
        }
    }

    // Try X-Forwarded-For
    if let Some(values) = req.headers.get("x-forwarded-for") {
        if let Some(value) = values.first() {
            for part in value.split(',') {
                let trimmed = part.trim();
                let (ip, ver) = iputil::parse_ip(trimmed);
                if let Some(ref ip) = ip {
                    if validator.is_valid(ip, ver) {
                        return (Some(*ip), ver);
                    }
                }
            }
        }
    }

    // Try X-Real-IP
    if let Some(values) = req.headers.get("x-real-ip") {
        if let Some(value) = values.first() {
            let trimmed = value.trim();
            let (ip, ver) = iputil::parse_ip(trimmed);
            if let Some(ref ip) = ip {
                if validator.is_valid(ip, ver) {
                    return (Some(*ip), ver);
                }
            }
        }
    }

    // Try RemoteAddr (split host:port)
    if !req.remote_addr.is_empty() {
        let host = if let Some(bracket_end) = req.remote_addr.find(']') {
            // IPv6 with brackets: [::1]:port
            &req.remote_addr[1..bracket_end]
        } else if let Some(colon_pos) = req.remote_addr.rfind(':') {
            // IPv4 with port: 1.2.3.4:port
            &req.remote_addr[..colon_pos]
        } else {
            &req.remote_addr
        };
        let (ip, ver) = iputil::parse_ip(host);
        if let Some(ref ip) = ip {
            if validator.is_valid(ip, ver) {
                return (Some(*ip), ver);
            }
        }
    }

    (None, IpVersion::Unknown)
}

/// Parse a query string into a HashMap.
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
    url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(pairs)
        .finish()
}

/// Check whether a string is a syntactically valid URL (has scheme + host).
pub fn is_valid_url(s: &str) -> bool {
    match url::Url::parse(s) {
        Ok(u) => u.has_host(),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct AcceptAll;
    impl IpValidator for AcceptAll {
        fn is_valid(&self, _ip: &IpAddr, _ver: IpVersion) -> bool {
            true
        }
    }

    #[test]
    fn test_find_ip_true_client_ip() {
        let req = RequestHeaders {
            headers: HashMap::from([
                ("true-client-ip".to_string(), vec!["1.2.3.4".to_string()]),
            ]),
            remote_addr: String::new(),
        };
        let (ip, ver) = find_ip(&req, &AcceptAll);
        assert!(ip.is_some());
        assert_eq!(ver, IpVersion::V4);
    }

    #[test]
    fn test_find_ip_x_forwarded_for() {
        let req = RequestHeaders {
            headers: HashMap::from([
                ("x-forwarded-for".to_string(), vec!["10.0.0.1, 1.2.3.4".to_string()]),
            ]),
            remote_addr: String::new(),
        };
        let (ip, _) = find_ip(&req, &AcceptAll);
        assert!(ip.is_some());
    }

    #[test]
    fn test_parse_query_string() {
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
    fn test_encode_url_params() {
        let mut params = HashMap::new();
        params.insert("foo".to_string(), "bar".to_string());
        params.insert("baz".to_string(), "hello world".to_string());
        let encoded = encode_url_params(&params);
        assert_eq!(encoded, "baz=hello+world&foo=bar");
    }

    #[test]
    fn test_is_valid_url() {
        assert!(is_valid_url("https://example.com"));
        assert!(!is_valid_url("not a url"));
        assert!(!is_valid_url(""));
    }
}
