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

/// HTTP-based prebid cache
pub struct HttpCache {
    pub base_url: String,
    pub client: reqwest::Client,
}
impl HttpCache {
    pub fn new(base_url: String) -> Self {
        Self { base_url, client: reqwest::Client::new() }
    }
}
impl CacheClient for HttpCache {
    fn put_json(&self, _: &serde_json::Value, _: u32) -> Option<String> { None }
    fn get_url(&self, uuid: &str) -> String {
        format!("{}/cache?uuid={}", self.base_url, uuid)
    }
}
