//! Filesystem-backed eager fetcher.
//!
//! Loads every `*.json` file under `stored_requests/` and `stored_imps/` at
//! startup into [`DashMap`]s keyed by the filename (without the `.json`
//! suffix).

use async_trait::async_trait;
use dashmap::DashMap;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::fetcher::{FetchError, Fetcher};

/// Eagerly-loaded filesystem fetcher.
pub struct FileFetcher {
    /// Root directory that was loaded from.
    pub root: PathBuf,
    /// Map of stored request ID -> parsed JSON.
    pub requests: DashMap<String, Value>,
    /// Map of stored imp ID -> parsed JSON.
    pub imps: DashMap<String, Value>,
    /// Map of stored response ID -> parsed JSON.
    pub responses: DashMap<String, Value>,
    /// Map of account ID -> parsed JSON.
    pub accounts: DashMap<String, Value>,
}

impl FileFetcher {
    /// Eagerly load all stored JSON under `root`.
    ///
    /// Expected layout:
    ///
    /// ```text
    /// root/
    ///   stored_requests/*.json
    ///   stored_imps/*.json
    ///   stored_responses/*.json   (optional)
    ///   accounts/*.json           (optional)
    /// ```
    pub fn new(root: impl AsRef<Path>) -> Result<Self, FetchError> {
        let root = root.as_ref().to_path_buf();

        let requests = load_dir(&root.join("stored_requests"))?;
        let imps = load_dir(&root.join("stored_imps"))?;
        // Responses and accounts are optional — absence is not an error.
        let responses = load_dir_optional(&root.join("stored_responses"))?;
        let accounts = load_dir_optional(&root.join("accounts"))?;

        Ok(Self {
            root,
            requests,
            imps,
            responses,
            accounts,
        })
    }
}

fn load_dir(dir: &Path) -> Result<DashMap<String, Value>, FetchError> {
    let map: DashMap<String, Value> = DashMap::new();
    if !dir.exists() {
        return Ok(map);
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("json") {
            let id = match path.file_stem().and_then(|s| s.to_str()) {
                Some(s) => s.to_string(),
                None => continue,
            };
            let bytes = fs::read(&path)?;
            let value: Value = serde_json::from_slice(&bytes)?;
            map.insert(id, value);
        }
    }
    Ok(map)
}

fn load_dir_optional(dir: &Path) -> Result<DashMap<String, Value>, FetchError> {
    if dir.exists() {
        load_dir(dir)
    } else {
        Ok(DashMap::new())
    }
}

fn collect(map: &DashMap<String, Value>, ids: &[String], data_type: &str) -> (HashMap<String, Value>, Vec<FetchError>) {
    let mut out = HashMap::with_capacity(ids.len());
    let mut errs = Vec::new();
    for id in ids {
        if let Some(v) = map.get(id) {
            out.insert(id.clone(), v.clone());
        } else {
            errs.push(FetchError::not_found(id.clone(), data_type));
        }
    }
    (out, errs)
}

#[async_trait]
impl Fetcher for FileFetcher {
    async fn fetch_requests(
        &self,
        req_ids: &[String],
        imp_ids: &[String],
    ) -> (
        HashMap<String, Value>,
        HashMap<String, Value>,
        Vec<FetchError>,
    ) {
        let (req, mut errs) = collect(&self.requests, req_ids, "Request");
        let (imp, imp_errs) = collect(&self.imps, imp_ids, "Imp");
        errs.extend(imp_errs);
        (req, imp, errs)
    }

    async fn fetch_account(&self, account_id: &str) -> Result<Value, FetchError> {
        if account_id.is_empty() {
            return Err(FetchError::Other("Cannot look up an empty accountID".into()));
        }
        self.accounts
            .get(account_id)
            .map(|v| v.clone())
            .ok_or_else(|| FetchError::not_found(account_id, "Account"))
    }

    async fn fetch_categories(
        &self,
        primary_adserver: &str,
        publisher_id: &str,
    ) -> Result<String, FetchError> {
        // Minimal implementation: look up a JSON file at
        // `{root}/{primary_adserver}/{primary_adserver}[_{publisher}].json`
        // and return its raw contents as a string.
        let file_name = if publisher_id.is_empty() {
            format!("{primary_adserver}.json")
        } else {
            format!("{primary_adserver}_{publisher_id}.json")
        };
        let path = self.root.join(primary_adserver).join(file_name);
        if !path.exists() {
            return Err(FetchError::not_found(
                format!("{primary_adserver}/{publisher_id}"),
                "Category",
            ));
        }
        let bytes = fs::read(&path)?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }

    async fn fetch_responses(
        &self,
        ids: &[String],
    ) -> Result<HashMap<String, Value>, FetchError> {
        let (out, errs) = collect(&self.responses, ids, "Response");
        if !errs.is_empty() {
            // Mirror Go behaviour: the responses map itself is still useful
            // even if some IDs are missing. Log and return what we have.
            for err in &errs {
                tracing::debug!(%err, "stored response not found");
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[tokio::test]
    async fn loads_requests_and_imps_from_tempdir() {
        let dir = tempdir().unwrap();
        let req_dir = dir.path().join("stored_requests");
        let imp_dir = dir.path().join("stored_imps");
        fs::create_dir_all(&req_dir).unwrap();
        fs::create_dir_all(&imp_dir).unwrap();

        let mut f = fs::File::create(req_dir.join("req-1.json")).unwrap();
        f.write_all(br#"{"hello":"world"}"#).unwrap();

        let mut f = fs::File::create(imp_dir.join("imp-1.json")).unwrap();
        f.write_all(br#"{"imp":true}"#).unwrap();

        let fetcher = FileFetcher::new(dir.path()).unwrap();
        let (req, imp, errs) = fetcher
            .fetch_requests(
                &["req-1".to_string(), "missing".to_string()],
                &["imp-1".to_string()],
            )
            .await;

        assert_eq!(req.len(), 1);
        assert_eq!(req.get("req-1").unwrap()["hello"], "world");
        assert_eq!(imp.len(), 1);
        assert_eq!(imp.get("imp-1").unwrap()["imp"], true);
        assert_eq!(errs.len(), 1);
    }
}
