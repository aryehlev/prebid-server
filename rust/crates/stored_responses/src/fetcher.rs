use std::collections::HashMap;

use async_trait::async_trait;
use serde_json::Value;

use crate::error::StoredRespError;

/// Pluggable backend for resolving stored response ids to their JSON bodies.
#[async_trait]
pub trait StoredResponsesFetcher: Send + Sync {
    /// Look up the supplied stored-response ids and return a map of id -> body.
    async fn fetch_responses(
        &self,
        ids: &[String],
    ) -> Result<HashMap<String, Value>, StoredRespError>;
}

/// Simple in-memory fetcher backed by a static map; useful for tests.
pub struct InMemoryStoredResponsesFetcher {
    pub responses: HashMap<String, Value>,
}

impl InMemoryStoredResponsesFetcher {
    pub fn new(responses: HashMap<String, Value>) -> Self {
        Self { responses }
    }
}

#[async_trait]
impl StoredResponsesFetcher for InMemoryStoredResponsesFetcher {
    async fn fetch_responses(
        &self,
        ids: &[String],
    ) -> Result<HashMap<String, Value>, StoredRespError> {
        let mut out = HashMap::with_capacity(ids.len());
        for id in ids {
            if let Some(body) = self.responses.get(id) {
                out.insert(id.clone(), body.clone());
            }
        }
        Ok(out)
    }
}
