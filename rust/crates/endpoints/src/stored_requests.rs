use std::collections::HashMap;

/// Simple in-memory stored request cache loaded from filesystem
pub struct StoredRequestFetcher {
    /// Map from request ID to stored BidRequest JSON
    requests: HashMap<String, serde_json::Value>,
}

impl StoredRequestFetcher {
    /// Load stored requests from a directory
    /// Looks for JSON files named <id>.json
    pub fn from_directory(dir: &str) -> Self {
        let mut requests = HashMap::new();
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("json") {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                                requests.insert(stem.to_string(), json);
                            }
                        }
                    }
                }
            }
        }
        Self { requests }
    }

    pub fn empty() -> Self {
        Self { requests: HashMap::new() }
    }

    pub fn get(&self, id: &str) -> Option<&serde_json::Value> {
        self.requests.get(id)
    }
}
