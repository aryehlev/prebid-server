use serde::{Deserialize, Serialize};

/// Prebid cache stores bid creatives/nurl for retrieval
pub trait CacheClient: Send + Sync {
    fn put_json(&self, value: &serde_json::Value, ttl_secs: u32) -> Option<String>;
    fn get_url(&self, uuid: &str) -> String;
}

/// No-op cache (default)
pub struct NoopCache;
impl CacheClient for NoopCache {
    fn put_json(&self, _: &serde_json::Value, _: u32) -> Option<String> { None }
    fn get_url(&self, _: &str) -> String { String::new() }
}

/// Request payload sent to Prebid Cache /cache endpoint
#[derive(Debug, Serialize)]
struct CachePutRequest {
    puts: Vec<CachePutObject>,
}

#[derive(Debug, Serialize)]
struct CachePutObject {
    #[serde(rename = "type")]
    content_type: String,
    value: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    ttlseconds: Option<u32>,
}

/// Response from Prebid Cache /cache endpoint
#[derive(Debug, Deserialize)]
struct CachePutResponse {
    responses: Vec<CachePutResponseObject>,
}

#[derive(Debug, Deserialize)]
struct CachePutResponseObject {
    uuid: String,
}

/// HTTP-based prebid cache
pub struct HttpCache {
    pub base_url: String,
    pub client: reqwest::Client,
}
impl HttpCache {
    pub fn new(base_url: String) -> Self {
        Self { base_url, client: reqwest::Client::new() }
    }

    /// Async implementation of cache put
    async fn put_json_async(&self, value: &serde_json::Value, ttl_secs: u32) -> Option<String> {
        let payload = CachePutRequest {
            puts: vec![CachePutObject {
                content_type: "json".to_string(),
                value: value.clone(),
                ttlseconds: if ttl_secs > 0 { Some(ttl_secs) } else { None },
            }],
        };

        let url = format!("{}/cache", self.base_url);
        let resp = self.client.post(&url).json(&payload).send().await.ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let body: CachePutResponse = resp.json().await.ok()?;
        body.responses.into_iter().next().map(|r| r.uuid)
    }
}

impl CacheClient for HttpCache {
    fn put_json(&self, value: &serde_json::Value, ttl_secs: u32) -> Option<String> {
        // Use tokio::task::block_in_place to run the async call synchronously
        // within the current tokio runtime context
        let value = value.clone();
        let ttl = ttl_secs;
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(self.put_json_async(&value, ttl))
        })
    }
    fn get_url(&self, uuid: &str) -> String {
        format!("{}/cache?uuid={}", self.base_url, uuid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_noop_cache() {
        let cache = NoopCache;
        assert_eq!(cache.put_json(&serde_json::json!({"a": 1}), 300), None);
        assert_eq!(cache.get_url("abc"), "");
    }

    #[test]
    fn test_http_cache_get_url() {
        let cache = HttpCache::new("https://cache.example.com".to_string());
        assert_eq!(
            cache.get_url("some-uuid"),
            "https://cache.example.com/cache?uuid=some-uuid"
        );
    }

    #[test]
    fn test_cache_put_request_serialization() {
        let req = CachePutRequest {
            puts: vec![CachePutObject {
                content_type: "json".to_string(),
                value: serde_json::json!({"bid": "data"}),
                ttlseconds: Some(300),
            }],
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["puts"][0]["type"], "json");
        assert_eq!(json["puts"][0]["ttlseconds"], 300);
    }

    #[test]
    fn test_cache_put_request_no_ttl() {
        let req = CachePutRequest {
            puts: vec![CachePutObject {
                content_type: "json".to_string(),
                value: serde_json::json!(null),
                ttlseconds: None,
            }],
        };
        let json = serde_json::to_value(&req).unwrap();
        assert!(json["puts"][0].get("ttlseconds").is_none());
    }
}
