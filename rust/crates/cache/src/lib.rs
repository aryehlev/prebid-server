use std::fmt;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Error type for Prebid Cache operations.
#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("error creating request to Prebid Cache: {0}")]
    RequestBuild(String),
    #[error("error sending request to Prebid Cache: {0}")]
    RequestSend(#[from] reqwest::Error),
    #[error("Prebid Cache returned HTTP {status}: {body}")]
    BadStatus { status: u16, body: String },
    #[error("error parsing Prebid Cache response: {0}")]
    ResponseParse(String),
    #[error("error encoding values for Prebid Cache: {0}")]
    Encode(#[from] serde_json::Error),
}

/// The type of payload being cached.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PayloadType {
    #[serde(rename = "json")]
    Json,
    #[serde(rename = "xml")]
    Xml,
}

impl fmt::Display for PayloadType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PayloadType::Json => write!(f, "json"),
            PayloadType::Xml => write!(f, "xml"),
        }
    }
}

/// An item to be stored in Prebid Cache.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cacheable {
    /// The type of payload (json or xml).
    #[serde(rename = "type")]
    pub payload_type: PayloadType,

    /// The raw value to cache.
    pub value: serde_json::Value,

    /// TTL in seconds. If 0 or negative, omitted from the request.
    #[serde(rename = "ttlseconds", skip_serializing_if = "is_zero_or_negative")]
    pub ttl_seconds: i64,

    /// Optional cache key.
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub key: String,

    /// Bid ID (used by /vtrack).
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub bidid: String,

    /// Bidder name (used by /vtrack).
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub bidder: String,

    /// Timestamp (used by /vtrack). If 0, omitted from the request.
    #[serde(skip_serializing_if = "is_zero_i64", default)]
    pub timestamp: i64,
}

fn is_zero_or_negative(v: &i64) -> bool {
    *v <= 0
}

fn is_zero_i64(v: &i64) -> bool {
    *v == 0
}

/// The request body sent to Prebid Cache.
#[derive(Debug, Clone, Serialize)]
struct PutRequest {
    puts: Vec<Cacheable>,
}

/// A single response entry from Prebid Cache.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheResponseObject {
    pub uuid: String,
}

/// The response body from Prebid Cache.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheResponse {
    pub responses: Vec<CacheResponseObject>,
}

/// Data about the externally-accessible cache URL.
#[derive(Debug, Clone, Default)]
pub struct ExtCacheData {
    pub scheme: String,
    pub host: String,
    pub path: String,
}

/// Trait for interacting with Prebid Cache.
///
/// Implementors can store bid data in a cache and retrieve the external cache
/// URL information needed to build cache IDs for bid responses.
#[async_trait]
pub trait Cache: Send + Sync {
    /// Store the given cacheable values in Prebid Cache.
    ///
    /// Returns a vector of UUIDs with the same length as `values`. If a value
    /// could not be cached, its corresponding entry will be an empty string.
    /// Any errors encountered are returned in the second element.
    async fn put(&self, values: Vec<Cacheable>) -> (Vec<String>, Vec<CacheError>);

    /// Get the scheme, host, and path of the externally accessible cache URL.
    fn get_ext_cache_data(&self) -> ExtCacheData;
}

/// Trait for recording cache metrics.
///
/// Mirrors Go's `metrics.MetricsEngine.RecordPrebidCacheRequestTime`.
pub trait CacheMetrics: Send + Sync {
    fn record_prebid_cache_request_time(&self, success: bool, elapsed: std::time::Duration);
}

/// No-op metrics implementation.
pub struct NoopCacheMetrics;

impl CacheMetrics for NoopCacheMetrics {
    fn record_prebid_cache_request_time(&self, _success: bool, _elapsed: std::time::Duration) {}
}

/// A real Prebid Cache client that communicates over HTTP.
#[derive(Debug, Clone)]
pub struct CacheClient {
    http_client: reqwest::Client,
    put_url: String,
    external_cache: ExtCacheData,
}

impl CacheClient {
    /// Create a new CacheClient.
    ///
    /// - `http_client`: the reqwest client to use for HTTP calls.
    /// - `cache_base_url`: the base URL for the Prebid Cache instance (e.g. "https://cache.example.com").
    /// - `external_cache`: the externally-accessible cache URL components.
    pub fn new(
        http_client: reqwest::Client,
        cache_base_url: &str,
        external_cache: ExtCacheData,
    ) -> Self {
        let put_url = format!("{}/cache", cache_base_url.trim_end_matches('/'));
        Self {
            http_client,
            put_url,
            external_cache,
        }
    }
}

#[async_trait]
impl Cache for CacheClient {
    async fn put(&self, values: Vec<Cacheable>) -> (Vec<String>, Vec<CacheError>) {
        let mut errors = Vec::new();

        if values.is_empty() {
            return (Vec::new(), errors);
        }

        let num_values = values.len();
        let uuids = vec![String::new(); num_values];

        let request_body = PutRequest { puts: values };
        let payload = match serde_json::to_vec(&request_body) {
            Ok(p) => p,
            Err(e) => {
                tracing::error!("Error creating JSON for Prebid Cache: {}", e);
                errors.push(CacheError::Encode(e));
                return (uuids, errors);
            }
        };

        let payload_size = payload.len();
        let start = std::time::Instant::now();

        let response = match self
            .http_client
            .post(&self.put_url)
            .header("Content-Type", "application/json;charset=utf-8")
            .header("Accept", "application/json")
            .body(payload)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                let elapsed = start.elapsed();
                tracing::error!(
                    "Error sending request to Prebid Cache: {}; Duration={:?}, Items={}, Payload Size={}",
                    e,
                    elapsed,
                    num_values,
                    payload_size,
                );
                errors.push(CacheError::RequestSend(e));
                return (uuids, errors);
            }
        };

        let status = response.status();
        let body = match response.text().await {
            Ok(b) => b,
            Err(e) => {
                tracing::error!("Error reading Prebid Cache response body: {}", e);
                errors.push(CacheError::RequestSend(e));
                return (uuids, errors);
            }
        };

        if !status.is_success() {
            tracing::error!(
                "Prebid Cache call to {} returned {}: {}",
                self.put_url,
                status.as_u16(),
                body
            );
            errors.push(CacheError::BadStatus {
                status: status.as_u16(),
                body,
            });
            return (uuids, errors);
        }

        let cache_response: CacheResponse = match serde_json::from_str(&body) {
            Ok(r) => r,
            Err(e) => {
                tracing::error!(
                    "Error interpreting Prebid Cache response: {}\nResponse was: {}",
                    e,
                    body
                );
                errors.push(CacheError::ResponseParse(format!(
                    "{}: body was: {}",
                    e, body
                )));
                return (uuids, errors);
            }
        };

        let mut result_uuids = Vec::with_capacity(num_values);
        for (i, resp) in cache_response.responses.into_iter().enumerate() {
            if i < num_values {
                result_uuids.push(resp.uuid);
            }
        }

        // Pad with empty strings if the response had fewer entries than expected
        while result_uuids.len() < num_values {
            let idx = result_uuids.len();
            tracing::error!(
                "Prebid Cache response missing entry at index {}. Response body: {}",
                idx,
                body
            );
            errors.push(CacheError::ResponseParse(format!(
                "missing response at index {}",
                idx
            )));
            result_uuids.push(String::new());
        }

        (result_uuids, errors)
    }

    fn get_ext_cache_data(&self) -> ExtCacheData {
        let mut path = self.external_cache.path.clone();
        if path == "/" {
            path = String::new();
        } else if !path.is_empty() && !path.starts_with('/') {
            path = format!("/{}", path);
        }

        ExtCacheData {
            scheme: self.external_cache.scheme.clone(),
            host: self.external_cache.host.clone(),
            path,
        }
    }
}

/// A cache client wrapper that records metrics on every put call.
///
/// Mirrors Go's metrics integration in `prebid_cache_client/client.go`.
pub struct CacheClientWithMetrics<M: CacheMetrics> {
    inner: CacheClient,
    metrics: M,
}

impl<M: CacheMetrics> CacheClientWithMetrics<M> {
    pub fn new(inner: CacheClient, metrics: M) -> Self {
        Self { inner, metrics }
    }
}

#[async_trait]
impl<M: CacheMetrics + 'static> Cache for CacheClientWithMetrics<M> {
    async fn put(&self, values: Vec<Cacheable>) -> (Vec<String>, Vec<CacheError>) {
        let start = std::time::Instant::now();
        let (uuids, errors) = self.inner.put(values).await;
        let elapsed = start.elapsed();
        let success = errors.is_empty();
        self.metrics.record_prebid_cache_request_time(success, elapsed);
        (uuids, errors)
    }

    fn get_ext_cache_data(&self) -> ExtCacheData {
        self.inner.get_ext_cache_data()
    }
}

/// A no-op cache client that does nothing.
///
/// Returns empty UUIDs for all items and empty external cache data.
/// Useful for testing or when caching is disabled.
#[derive(Debug, Clone, Default)]
pub struct NoopCacheClient;

#[async_trait]
impl Cache for NoopCacheClient {
    async fn put(&self, values: Vec<Cacheable>) -> (Vec<String>, Vec<CacheError>) {
        let uuids = vec![String::new(); values.len()];
        (uuids, Vec::new())
    }

    fn get_ext_cache_data(&self) -> ExtCacheData {
        ExtCacheData::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_payload_type_display() {
        assert_eq!(PayloadType::Json.to_string(), "json");
        assert_eq!(PayloadType::Xml.to_string(), "xml");
    }

    #[test]
    fn test_cacheable_serialization() {
        let item = Cacheable {
            payload_type: PayloadType::Json,
            value: serde_json::json!({}),
            ttl_seconds: 300,
            key: String::new(),
            bidid: "bid".to_string(),
            bidder: "bdr".to_string(),
            timestamp: 123456789,
        };

        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains(r#""type":"json""#));
        assert!(json.contains(r#""ttlseconds":300"#));
        assert!(json.contains(r#""value":{}"#));
        assert!(json.contains(r#""bidid":"bid""#));
        assert!(json.contains(r#""bidder":"bdr""#));
        assert!(json.contains(r#""timestamp":123456789"#));
        // key is empty, should be omitted
        assert!(!json.contains(r#""key""#));
    }

    #[test]
    fn test_cacheable_omits_zero_ttl() {
        let item = Cacheable {
            payload_type: PayloadType::Json,
            value: serde_json::json!(true),
            ttl_seconds: 0,
            key: String::new(),
            bidid: String::new(),
            bidder: String::new(),
            timestamp: 0,
        };

        let json = serde_json::to_string(&item).unwrap();
        assert!(!json.contains("ttlseconds"));
        assert!(!json.contains("timestamp"));
        assert!(!json.contains("bidid"));
        assert!(!json.contains("bidder"));
        assert!(!json.contains("key"));
    }

    #[test]
    fn test_put_request_serialization() {
        let req = PutRequest {
            puts: vec![
                Cacheable {
                    payload_type: PayloadType::Json,
                    value: serde_json::json!(true),
                    ttl_seconds: 300,
                    key: String::new(),
                    bidid: String::new(),
                    bidder: String::new(),
                    timestamp: 0,
                },
                Cacheable {
                    payload_type: PayloadType::Xml,
                    value: serde_json::json!("<vast></vast>"),
                    ttl_seconds: 0,
                    key: String::new(),
                    bidid: String::new(),
                    bidder: String::new(),
                    timestamp: 0,
                },
            ],
        };

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.starts_with(r#"{"puts":["#));
        assert!(json.contains(r#""type":"json""#));
        assert!(json.contains(r#""type":"xml""#));
    }

    #[test]
    fn test_cache_response_deserialization() {
        let body = r#"{"responses":[{"uuid":"abc-123"},{"uuid":"def-456"}]}"#;
        let resp: CacheResponse = serde_json::from_str(body).unwrap();
        assert_eq!(resp.responses.len(), 2);
        assert_eq!(resp.responses[0].uuid, "abc-123");
        assert_eq!(resp.responses[1].uuid, "def-456");
    }

    #[test]
    fn test_ext_cache_data_path_normalization() {
        // Path is "/"  -> normalized to ""
        let client = CacheClient::new(
            reqwest::Client::new(),
            "https://cache.example.com",
            ExtCacheData {
                scheme: "https".to_string(),
                host: "cache.example.com".to_string(),
                path: "/".to_string(),
            },
        );
        let data = client.get_ext_cache_data();
        assert_eq!(data.path, "");

        // Path without leading slash -> prepend "/"
        let client = CacheClient::new(
            reqwest::Client::new(),
            "https://cache.example.com",
            ExtCacheData {
                scheme: "https".to_string(),
                host: "cache.example.com".to_string(),
                path: "pbcache/endpoint".to_string(),
            },
        );
        let data = client.get_ext_cache_data();
        assert_eq!(data.path, "/pbcache/endpoint");

        // Path with leading slash -> kept as-is
        let client = CacheClient::new(
            reqwest::Client::new(),
            "https://cache.example.com",
            ExtCacheData {
                scheme: "".to_string(),
                host: "prebid-server.prebid.org".to_string(),
                path: "/pbcache/endpoint".to_string(),
            },
        );
        let data = client.get_ext_cache_data();
        assert_eq!(data.path, "/pbcache/endpoint");

        // Empty path -> stays empty
        let client = CacheClient::new(
            reqwest::Client::new(),
            "https://cache.example.com",
            ExtCacheData {
                scheme: "".to_string(),
                host: "prebidcache.net".to_string(),
                path: "".to_string(),
            },
        );
        let data = client.get_ext_cache_data();
        assert_eq!(data.path, "");
    }

    #[tokio::test]
    async fn test_noop_cache_client_empty() {
        let client = NoopCacheClient;
        let (ids, errs) = client.put(vec![]).await;
        assert!(ids.is_empty());
        assert!(errs.is_empty());
    }

    #[tokio::test]
    async fn test_noop_cache_client_with_values() {
        let client = NoopCacheClient;
        let values = vec![
            Cacheable {
                payload_type: PayloadType::Json,
                value: serde_json::json!(true),
                ttl_seconds: 0,
                key: String::new(),
                bidid: String::new(),
                bidder: String::new(),
                timestamp: 0,
            },
            Cacheable {
                payload_type: PayloadType::Json,
                value: serde_json::json!(false),
                ttl_seconds: 0,
                key: String::new(),
                bidid: String::new(),
                bidder: String::new(),
                timestamp: 0,
            },
        ];
        let (ids, errs) = client.put(values).await;
        assert_eq!(ids.len(), 2);
        assert_eq!(ids[0], "");
        assert_eq!(ids[1], "");
        assert!(errs.is_empty());
    }

    #[tokio::test]
    async fn test_noop_cache_client_ext_data() {
        let client = NoopCacheClient;
        let data = client.get_ext_cache_data();
        assert_eq!(data.scheme, "");
        assert_eq!(data.host, "");
        assert_eq!(data.path, "");
    }

    #[tokio::test]
    async fn test_cache_client_empty_put() {
        let client = CacheClient::new(
            reqwest::Client::new(),
            "https://cache.example.com",
            ExtCacheData::default(),
        );
        let (ids, errs) = client.put(vec![]).await;
        assert!(ids.is_empty());
        assert!(errs.is_empty());
    }

    #[test]
    fn test_cache_client_put_url_construction() {
        let client = CacheClient::new(
            reqwest::Client::new(),
            "https://cache.example.com",
            ExtCacheData::default(),
        );
        assert_eq!(client.put_url, "https://cache.example.com/cache");

        // With trailing slash
        let client = CacheClient::new(
            reqwest::Client::new(),
            "https://cache.example.com/",
            ExtCacheData::default(),
        );
        assert_eq!(client.put_url, "https://cache.example.com/cache");
    }
}
