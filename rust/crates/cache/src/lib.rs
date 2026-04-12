//! Prebid Cache client.
//!
//! This crate is a Rust port of
//! `github.com/prebid/prebid-server/prebid_cache_client`. It exposes a
//! [`Cache`] trait backed by either a real [`Client`] that speaks to a
//! Prebid Cache HTTP endpoint, or a [`NoopCache`] stub used for tests and
//! when caching is disabled.
//!
//! The wire format matches Go's `encodeValues` output: a JSON object with a
//! `puts` array of entries, each serialized using the keys `type`, `value`,
//! `ttlseconds`, `key`, `bidid`, `bidder`, and `timestamp`.

use std::fmt;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Errors returned by Prebid Cache operations.
#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    /// The HTTP request could not be constructed (e.g. bad URL).
    #[error("error creating request to Prebid Cache: {0}")]
    RequestBuild(String),

    /// Sending the HTTP request or reading the response failed.
    #[error("error sending request to Prebid Cache: {0}")]
    RequestSend(#[from] reqwest::Error),

    /// Prebid Cache returned a non-2xx HTTP status.
    #[error("Prebid Cache returned HTTP {status}: {body}")]
    BadStatus { status: u16, body: String },

    /// The response body could not be parsed as the expected format.
    #[error("error parsing Prebid Cache response: {0}")]
    ResponseParse(String),

    /// Encoding the outgoing request body failed.
    #[error("error encoding values for Prebid Cache: {0}")]
    Encode(#[from] serde_json::Error),
}

/// The type of payload being cached. Mirrors Go's `PayloadType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PayloadType {
    Json,
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

/// A single item to be stored in Prebid Cache.
///
/// Mirrors the Go `Cacheable` struct. The `data` field contains the raw JSON
/// value that will be stored under the `value` key on the wire, matching the
/// JSON shape used by the Go client.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Cacheable {
    /// Payload type (`json` or `xml`).
    #[serde(rename = "type")]
    pub r#type: Option<PayloadType>,

    /// The raw value to cache. Serialized on the wire as `value`.
    #[serde(rename = "value", skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,

    /// TTL in seconds. Values `<= 0` are omitted from the request.
    #[serde(
        rename = "ttlseconds",
        skip_serializing_if = "is_zero_or_negative",
        default
    )]
    pub ttl_seconds: i64,

    /// Optional cache key.
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub key: String,

    /// Bid ID (used by `/vtrack`).
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub bidid: String,

    /// Bidder name (used by `/vtrack`).
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub bidder: String,

    /// Timestamp in ms (used by `/vtrack`). `0` is omitted.
    #[serde(skip_serializing_if = "is_zero_i64", default)]
    pub timestamp: i64,
}

fn is_zero_or_negative(v: &i64) -> bool {
    *v <= 0
}

fn is_zero_i64(v: &i64) -> bool {
    *v == 0
}

/// Request body sent to Prebid Cache on PUT (POST `/cache`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PutRequest {
    pub puts: Vec<Cacheable>,
}

/// A single response entry returned by Prebid Cache.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheResponseObject {
    pub uuid: String,
}

/// Response body returned by Prebid Cache.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PutResponse {
    pub responses: Vec<CacheResponseObject>,
}

/// Data about the externally-accessible cache URL used by clients to build
/// cache lookup URLs in bid responses.
#[derive(Debug, Clone, Default)]
pub struct ExtCacheData {
    pub scheme: String,
    pub host: String,
    pub path: String,
}

/// Trait for interacting with Prebid Cache.
///
/// Ports Go's `Client` interface. Implementors can persist bid data and
/// return the UUIDs assigned by the cache, plus report the externally
/// accessible cache URL components used to build cache lookup URLs.
#[async_trait]
pub trait Cache: Send + Sync {
    /// Store the given cacheable values in Prebid Cache.
    ///
    /// The returned vector always has the same number of entries as
    /// `values`. When an individual entry could not be stored the
    /// corresponding element is an empty string, matching Go's `PutJson`
    /// contract. Any errors encountered are returned in the second element.
    async fn put_objects(
        &self,
        values: Vec<Cacheable>,
    ) -> (Vec<String>, Vec<CacheError>);

    /// Get the scheme, host and path of the externally accessible cache URL.
    fn get_ext_cache_data(&self) -> ExtCacheData;
}

/// Configuration for building a [`Client`].
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Base URL of the Prebid Cache instance, e.g. `https://cache.example.com`.
    /// `/cache` is appended automatically to form the PUT endpoint.
    pub base_url: String,

    /// Timeout applied to each PUT request.
    pub timeout: Duration,

    /// External cache URL information returned from [`Cache::get_ext_cache_data`].
    pub external_cache: ExtCacheData,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            base_url: String::new(),
            timeout: Duration::from_secs(1),
            external_cache: ExtCacheData::default(),
        }
    }
}

/// Real Prebid Cache client that speaks HTTP. Ports Go's `clientImpl`.
#[derive(Debug, Clone)]
pub struct Client {
    http_client: reqwest::Client,
    put_url: String,
    timeout: Duration,
    external_cache: ExtCacheData,
}

impl Client {
    /// Construct a new [`Client`] from an existing `reqwest::Client` and a
    /// [`ClientConfig`].
    pub fn new(http_client: reqwest::Client, config: ClientConfig) -> Self {
        let put_url = format!("{}/cache", config.base_url.trim_end_matches('/'));
        Self {
            http_client,
            put_url,
            timeout: config.timeout,
            external_cache: config.external_cache,
        }
    }

    /// Convenience constructor that builds a new `reqwest::Client` from the
    /// supplied config.
    pub fn from_config(config: ClientConfig) -> Result<Self, CacheError> {
        let http_client = reqwest::Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(CacheError::RequestSend)?;
        Ok(Self::new(http_client, config))
    }

    /// Returns the fully-qualified PUT URL used by this client.
    pub fn put_url(&self) -> &str {
        &self.put_url
    }

    /// Returns the per-request timeout.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }
}

#[async_trait]
impl Cache for Client {
    async fn put_objects(
        &self,
        values: Vec<Cacheable>,
    ) -> (Vec<String>, Vec<CacheError>) {
        let mut errors: Vec<CacheError> = Vec::new();

        if values.is_empty() {
            return (Vec::new(), errors);
        }

        let num_values = values.len();
        let mut uuids = vec![String::new(); num_values];

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
            .timeout(self.timeout)
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

        let parsed: PutResponse = match serde_json::from_str(&body) {
            Ok(r) => r,
            Err(e) => {
                tracing::error!(
                    "Error interpreting Prebid Cache response: {}\nResponse was: {}",
                    e,
                    body
                );
                errors.push(CacheError::ResponseParse(format!(
                    "{e}: body was: {body}"
                )));
                return (uuids, errors);
            }
        };

        for (i, resp) in parsed.responses.into_iter().enumerate() {
            if i >= num_values {
                break;
            }
            uuids[i] = resp.uuid;
        }

        (uuids, errors)
    }

    fn get_ext_cache_data(&self) -> ExtCacheData {
        let mut path = self.external_cache.path.clone();
        if path == "/" {
            path.clear();
        } else if !path.is_empty() && !path.starts_with('/') {
            path = format!("/{path}");
        }

        ExtCacheData {
            scheme: self.external_cache.scheme.clone(),
            host: self.external_cache.host.clone(),
            path,
        }
    }
}

/// No-op cache client. Returns empty UUIDs and empty external cache data.
/// Useful for tests or when caching is disabled.
#[derive(Debug, Clone, Default)]
pub struct NoopCache;

#[async_trait]
impl Cache for NoopCache {
    async fn put_objects(
        &self,
        values: Vec<Cacheable>,
    ) -> (Vec<String>, Vec<CacheError>) {
        (vec![String::new(); values.len()], Vec::new())
    }

    fn get_ext_cache_data(&self) -> ExtCacheData {
        ExtCacheData::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(data: serde_json::Value) -> Cacheable {
        Cacheable {
            r#type: Some(PayloadType::Json),
            data: Some(data),
            ttl_seconds: 0,
            key: String::new(),
            bidid: String::new(),
            bidder: String::new(),
            timestamp: 0,
        }
    }

    #[test]
    fn payload_type_display_and_serde() {
        assert_eq!(PayloadType::Json.to_string(), "json");
        assert_eq!(PayloadType::Xml.to_string(), "xml");

        let s = serde_json::to_string(&PayloadType::Json).unwrap();
        assert_eq!(s, "\"json\"");
        let v: PayloadType = serde_json::from_str("\"xml\"").unwrap();
        assert_eq!(v, PayloadType::Xml);
    }

    #[test]
    fn cacheable_serialization_full() {
        let item = Cacheable {
            r#type: Some(PayloadType::Json),
            data: Some(serde_json::json!({})),
            ttl_seconds: 300,
            key: "cache-key".to_string(),
            bidid: "bid".to_string(),
            bidder: "bdr".to_string(),
            timestamp: 123456789,
        };

        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains(r#""type":"json""#));
        assert!(json.contains(r#""ttlseconds":300"#));
        assert!(json.contains(r#""value":{}"#));
        assert!(json.contains(r#""key":"cache-key""#));
        assert!(json.contains(r#""bidid":"bid""#));
        assert!(json.contains(r#""bidder":"bdr""#));
        assert!(json.contains(r#""timestamp":123456789"#));
    }

    #[test]
    fn cacheable_omits_empty_and_zero_fields() {
        let item = sample(serde_json::json!(true));
        let json = serde_json::to_string(&item).unwrap();
        assert!(!json.contains("ttlseconds"));
        assert!(!json.contains("timestamp"));
        assert!(!json.contains("bidid"));
        assert!(!json.contains("bidder"));
        assert!(!json.contains("key"));
        assert!(json.contains(r#""type":"json""#));
        assert!(json.contains(r#""value":true"#));
    }

    #[test]
    fn put_request_serialization() {
        let req = PutRequest {
            puts: vec![
                Cacheable {
                    r#type: Some(PayloadType::Json),
                    data: Some(serde_json::json!(true)),
                    ttl_seconds: 300,
                    ..Default::default()
                },
                Cacheable {
                    r#type: Some(PayloadType::Xml),
                    data: Some(serde_json::json!("<vast></vast>")),
                    ttl_seconds: 0,
                    ..Default::default()
                },
            ],
        };

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.starts_with(r#"{"puts":["#));
        assert!(json.contains(r#""type":"json""#));
        assert!(json.contains(r#""type":"xml""#));
    }

    #[test]
    fn put_response_deserialization() {
        let body = r#"{"responses":[{"uuid":"abc-123"},{"uuid":"def-456"}]}"#;
        let resp: PutResponse = serde_json::from_str(body).unwrap();
        assert_eq!(resp.responses.len(), 2);
        assert_eq!(resp.responses[0].uuid, "abc-123");
        assert_eq!(resp.responses[1].uuid, "def-456");
    }

    fn mk_client(path: &str) -> Client {
        Client::new(
            reqwest::Client::new(),
            ClientConfig {
                base_url: "https://cache.example.com".to_string(),
                timeout: Duration::from_secs(1),
                external_cache: ExtCacheData {
                    scheme: "https".to_string(),
                    host: "cache.example.com".to_string(),
                    path: path.to_string(),
                },
            },
        )
    }

    #[test]
    fn ext_cache_data_path_normalization() {
        assert_eq!(mk_client("/").get_ext_cache_data().path, "");
        assert_eq!(
            mk_client("pbcache/endpoint").get_ext_cache_data().path,
            "/pbcache/endpoint"
        );
        assert_eq!(
            mk_client("/pbcache/endpoint").get_ext_cache_data().path,
            "/pbcache/endpoint"
        );
        assert_eq!(mk_client("").get_ext_cache_data().path, "");
    }

    #[test]
    fn client_put_url_construction() {
        let c = Client::new(
            reqwest::Client::new(),
            ClientConfig {
                base_url: "https://cache.example.com".to_string(),
                timeout: Duration::from_secs(1),
                external_cache: ExtCacheData::default(),
            },
        );
        assert_eq!(c.put_url(), "https://cache.example.com/cache");

        let c = Client::new(
            reqwest::Client::new(),
            ClientConfig {
                base_url: "https://cache.example.com/".to_string(),
                timeout: Duration::from_secs(1),
                external_cache: ExtCacheData::default(),
            },
        );
        assert_eq!(c.put_url(), "https://cache.example.com/cache");
    }

    #[test]
    fn from_config_builds_client() {
        let c = Client::from_config(ClientConfig {
            base_url: "https://cache.example.com".to_string(),
            timeout: Duration::from_millis(250),
            external_cache: ExtCacheData::default(),
        })
        .expect("client should build");
        assert_eq!(c.put_url(), "https://cache.example.com/cache");
        assert_eq!(c.timeout(), Duration::from_millis(250));
    }

    #[tokio::test]
    async fn client_empty_put_is_noop() {
        let c = mk_client("");
        let (ids, errs) = c.put_objects(vec![]).await;
        assert!(ids.is_empty());
        assert!(errs.is_empty());
    }

    #[tokio::test]
    async fn noop_cache_empty() {
        let c = NoopCache;
        let (ids, errs) = c.put_objects(vec![]).await;
        assert!(ids.is_empty());
        assert!(errs.is_empty());
    }

    #[tokio::test]
    async fn noop_cache_with_values() {
        let c = NoopCache;
        let values = vec![
            sample(serde_json::json!(true)),
            sample(serde_json::json!(false)),
        ];
        let (ids, errs) = c.put_objects(values).await;
        assert_eq!(ids.len(), 2);
        assert!(ids.iter().all(|s| s.is_empty()));
        assert!(errs.is_empty());
    }

    #[tokio::test]
    async fn noop_cache_ext_data_is_default() {
        let c = NoopCache;
        let d = c.get_ext_cache_data();
        assert_eq!(d.scheme, "");
        assert_eq!(d.host, "");
        assert_eq!(d.path, "");
    }
}
