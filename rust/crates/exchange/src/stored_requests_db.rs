//! Database-backed stored request fetcher.
//!
//! Mirrors Go `stored_requests/backends/db_fetcher/fetcher.go` — fetches
//! stored requests, impressions, and responses from a database via a
//! pluggable `DbProvider` trait so users can plug in any async DB driver
//! (tokio-postgres, sqlx, etc.).

use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// DB row and query abstraction — mirrors Go db_provider.DbProvider
// ---------------------------------------------------------------------------

/// A single row result from a database query.
///
/// Each stored-request query is expected to return three columns:
/// `(id, data, data_type)` where `data` is a JSON blob and `data_type`
/// is either "request", "imp", or the response type marker.
#[derive(Debug, Clone)]
pub struct StoredDataRow {
    pub id: String,
    pub data: Value,
    pub data_type: String,
}

/// A named query parameter passed to a `DbProvider`.
///
/// The `value` is a list of string IDs; the provider is responsible for
/// substituting `{NAME}` in the query template with a properly-parameterized
/// list of placeholders.
#[derive(Debug, Clone)]
pub struct QueryParam {
    pub name: String,
    pub values: Vec<String>,
}

/// Errors returned by a DbProvider.
#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("database query error: {0}")]
    Query(String),
    #[error("bad input: {0}")]
    BadInput(String),
    #[error("connection error: {0}")]
    Connection(String),
}

/// Pluggable database provider interface.
///
/// Implementations substitute `{PARAM_NAME}` placeholders in `query_template`
/// with the parameter values and execute the query. The provider must be
/// async-safe and shareable across tasks.
///
/// Mirrors Go `stored_requests/backends/db_provider.DbProvider`.
#[async_trait]
pub trait DbProvider: Send + Sync {
    /// Execute a query with the given template and parameters.
    /// Returns rows, each with `(id, data, data_type)`.
    async fn query(
        &self,
        query_template: &str,
        params: &[QueryParam],
    ) -> Result<Vec<StoredDataRow>, DbError>;
}

// ---------------------------------------------------------------------------
// DbStoredRequestFetcher — the main database fetcher
// ---------------------------------------------------------------------------

/// Database-backed stored request fetcher.
///
/// Mirrors Go `db_fetcher.dbFetcher`. Accepts a `DbProvider` for the
/// underlying database driver (tokio-postgres, sqlx, mysql_async, etc.).
pub struct DbStoredRequestFetcher {
    provider: Arc<dyn DbProvider>,
    query_template: String,
    response_query_template: String,
}

impl DbStoredRequestFetcher {
    /// Create a new database-backed fetcher.
    ///
    /// - `provider`: the async DB provider to use
    /// - `query_template`: SQL query for fetching requests and imps, e.g.
    ///   `"SELECT id, data, type FROM stored_data WHERE id IN ({REQUEST_ID_LIST}) OR id IN ({IMP_ID_LIST})"`
    /// - `response_query_template`: SQL query for fetching stored responses
    pub fn new(
        provider: Arc<dyn DbProvider>,
        query_template: impl Into<String>,
        response_query_template: impl Into<String>,
    ) -> Self {
        Self {
            provider,
            query_template: query_template.into(),
            response_query_template: response_query_template.into(),
        }
    }

    /// Fetch stored requests and impressions by ID.
    ///
    /// Returns two maps: (requests, imps) keyed by ID. IDs not found in the
    /// database are reported as errors.
    ///
    /// Mirrors Go `dbFetcher.FetchRequests`.
    pub async fn fetch_requests(
        &self,
        request_ids: &[String],
        imp_ids: &[String],
    ) -> (HashMap<String, Value>, HashMap<String, Value>, Vec<String>) {
        let mut stored_requests = HashMap::new();
        let mut stored_imps = HashMap::new();
        let mut errors = Vec::new();

        if request_ids.is_empty() && imp_ids.is_empty() {
            return (stored_requests, stored_imps, errors);
        }

        let params = vec![
            QueryParam {
                name: "REQUEST_ID_LIST".to_string(),
                values: request_ids.to_vec(),
            },
            QueryParam {
                name: "IMP_ID_LIST".to_string(),
                values: imp_ids.to_vec(),
            },
        ];

        let rows = match self.provider.query(&self.query_template, &params).await {
            Ok(r) => r,
            Err(e) => {
                errors.push(format!("Error reading from Stored Request DB: {}", e));
                for id in request_ids {
                    errors.push(format!("No stored Request found for id: {}", id));
                }
                for id in imp_ids {
                    errors.push(format!("No stored Imp found for id: {}", id));
                }
                return (stored_requests, stored_imps, errors);
            }
        };

        for row in rows {
            match row.data_type.as_str() {
                "request" => {
                    stored_requests.insert(row.id, row.data);
                }
                "imp" => {
                    stored_imps.insert(row.id, row.data);
                }
                other => {
                    tracing::error!(
                        "Database result set with id={} has invalid type: {}. This will be ignored.",
                        row.id,
                        other
                    );
                }
            }
        }

        // Report missing IDs as errors
        for id in request_ids {
            if !stored_requests.contains_key(id) {
                errors.push(format!("No stored Request found for id: {}", id));
            }
        }
        for id in imp_ids {
            if !stored_imps.contains_key(id) {
                errors.push(format!("No stored Imp found for id: {}", id));
            }
        }

        (stored_requests, stored_imps, errors)
    }

    /// Fetch stored responses by ID.
    /// Mirrors Go `dbFetcher.FetchResponses`.
    pub async fn fetch_responses(
        &self,
        ids: &[String],
    ) -> (HashMap<String, Value>, Vec<String>) {
        let mut stored_data = HashMap::new();
        let mut errors = Vec::new();

        if ids.is_empty() {
            return (stored_data, errors);
        }

        let params = vec![QueryParam {
            name: "ID_LIST".to_string(),
            values: ids.to_vec(),
        }];

        let rows = match self
            .provider
            .query(&self.response_query_template, &params)
            .await
        {
            Ok(r) => r,
            Err(e) => {
                errors.push(format!("Error reading from Stored Response DB: {}", e));
                return (stored_data, errors);
            }
        };

        for row in rows {
            stored_data.insert(row.id, row.data);
        }

        (stored_data, errors)
    }
}

// ---------------------------------------------------------------------------
// Helper: substitute query parameters
// ---------------------------------------------------------------------------

/// Substitute `{PARAM_NAME}` placeholders in a query template with
/// a comma-separated list of positional placeholders (e.g., `$1, $2, $3`
/// for PostgreSQL or `?, ?, ?` for MySQL).
///
/// Returns the expanded query and a flat list of parameter values in order.
///
/// Used by concrete DbProvider implementations to turn the generic template
/// into a database-specific query.
pub fn expand_query_template(
    query: &str,
    params: &[QueryParam],
    placeholder_fn: impl Fn(usize) -> String,
) -> (String, Vec<String>) {
    let mut expanded = query.to_string();
    let mut values = Vec::new();
    let mut counter = 0;

    for param in params {
        let placeholder_name = format!("{{{}}}", param.name);
        let mut placeholders = Vec::with_capacity(param.values.len());

        for value in &param.values {
            counter += 1;
            placeholders.push(placeholder_fn(counter));
            values.push(value.clone());
        }

        let replacement = if placeholders.is_empty() {
            // Empty IN() lists are invalid — use NULL to match no rows
            "NULL".to_string()
        } else {
            placeholders.join(", ")
        };

        expanded = expanded.replace(&placeholder_name, &replacement);
    }

    (expanded, values)
}

/// PostgreSQL-style placeholder: `$1`, `$2`, etc.
pub fn postgres_placeholder(i: usize) -> String {
    format!("${}", i)
}

/// MySQL-style placeholder: `?`.
pub fn mysql_placeholder(_i: usize) -> String {
    "?".to_string()
}

// ---------------------------------------------------------------------------
// In-memory mock provider — for testing and development
// ---------------------------------------------------------------------------

/// An in-memory mock DB provider useful for testing.
///
/// Holds pre-populated data and returns matching rows based on the query
/// parameters (ignoring the query template itself).
pub struct MockDbProvider {
    pub data: HashMap<String, StoredDataRow>,
}

impl MockDbProvider {
    pub fn new() -> Self {
        Self { data: HashMap::new() }
    }

    pub fn insert(&mut self, id: impl Into<String>, data: Value, data_type: impl Into<String>) {
        let id = id.into();
        self.data.insert(
            id.clone(),
            StoredDataRow {
                id,
                data,
                data_type: data_type.into(),
            },
        );
    }
}

impl Default for MockDbProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DbProvider for MockDbProvider {
    async fn query(
        &self,
        _query_template: &str,
        params: &[QueryParam],
    ) -> Result<Vec<StoredDataRow>, DbError> {
        let mut results = Vec::new();
        for param in params {
            for id in &param.values {
                if let Some(row) = self.data.get(id) {
                    results.push(row.clone());
                }
            }
        }
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_mock_provider_returns_matching_rows() {
        let mut provider = MockDbProvider::new();
        provider.insert("req1", json!({"field": "value1"}), "request");
        provider.insert("imp1", json!({"field": "value2"}), "imp");

        let fetcher = DbStoredRequestFetcher::new(
            Arc::new(provider),
            "SELECT * WHERE id IN ({REQUEST_ID_LIST}) OR id IN ({IMP_ID_LIST})",
            "SELECT * WHERE id IN ({ID_LIST})",
        );

        let (reqs, imps, errs) = fetcher
            .fetch_requests(&["req1".to_string()], &["imp1".to_string()])
            .await;

        assert_eq!(reqs.len(), 1);
        assert_eq!(imps.len(), 1);
        assert_eq!(reqs["req1"], json!({"field": "value1"}));
        assert_eq!(imps["imp1"], json!({"field": "value2"}));
        assert!(errs.is_empty());
    }

    #[tokio::test]
    async fn test_missing_ids_reported_as_errors() {
        let provider = MockDbProvider::new();
        let fetcher = DbStoredRequestFetcher::new(
            Arc::new(provider),
            "SELECT * WHERE id IN ({REQUEST_ID_LIST})",
            "SELECT * WHERE id IN ({ID_LIST})",
        );

        let (reqs, imps, errs) = fetcher
            .fetch_requests(&["missing".to_string()], &[])
            .await;

        assert!(reqs.is_empty());
        assert!(imps.is_empty());
        assert_eq!(errs.len(), 1);
        assert!(errs[0].contains("missing"));
    }

    #[tokio::test]
    async fn test_empty_ids_returns_empty() {
        let provider = MockDbProvider::new();
        let fetcher = DbStoredRequestFetcher::new(
            Arc::new(provider),
            "SELECT * WHERE id IN ({REQUEST_ID_LIST})",
            "SELECT * WHERE id IN ({ID_LIST})",
        );

        let (reqs, imps, errs) = fetcher.fetch_requests(&[], &[]).await;
        assert!(reqs.is_empty());
        assert!(imps.is_empty());
        assert!(errs.is_empty());
    }

    #[tokio::test]
    async fn test_fetch_responses() {
        let mut provider = MockDbProvider::new();
        provider.insert("resp1", json!({"bid": "data"}), "response");

        let fetcher = DbStoredRequestFetcher::new(
            Arc::new(provider),
            "",
            "SELECT * WHERE id IN ({ID_LIST})",
        );

        let (data, errs) = fetcher.fetch_responses(&["resp1".to_string()]).await;
        assert_eq!(data.len(), 1);
        assert_eq!(data["resp1"], json!({"bid": "data"}));
        assert!(errs.is_empty());
    }

    #[test]
    fn test_expand_query_postgres() {
        let query = "SELECT * FROM t WHERE id IN ({IDS})";
        let params = vec![QueryParam {
            name: "IDS".to_string(),
            values: vec!["a".to_string(), "b".to_string(), "c".to_string()],
        }];
        let (expanded, values) = expand_query_template(query, &params, postgres_placeholder);
        assert_eq!(expanded, "SELECT * FROM t WHERE id IN ($1, $2, $3)");
        assert_eq!(values, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_expand_query_mysql() {
        let query = "SELECT * FROM t WHERE id IN ({IDS})";
        let params = vec![QueryParam {
            name: "IDS".to_string(),
            values: vec!["a".to_string(), "b".to_string()],
        }];
        let (expanded, values) = expand_query_template(query, &params, mysql_placeholder);
        assert_eq!(expanded, "SELECT * FROM t WHERE id IN (?, ?)");
        assert_eq!(values, vec!["a", "b"]);
    }

    #[test]
    fn test_expand_query_multiple_params() {
        let query = "SELECT * FROM t WHERE id IN ({REQ}) OR imp_id IN ({IMP})";
        let params = vec![
            QueryParam {
                name: "REQ".to_string(),
                values: vec!["r1".to_string(), "r2".to_string()],
            },
            QueryParam {
                name: "IMP".to_string(),
                values: vec!["i1".to_string()],
            },
        ];
        let (expanded, values) = expand_query_template(query, &params, postgres_placeholder);
        assert_eq!(
            expanded,
            "SELECT * FROM t WHERE id IN ($1, $2) OR imp_id IN ($3)"
        );
        assert_eq!(values, vec!["r1", "r2", "i1"]);
    }

    #[test]
    fn test_expand_query_empty_list_uses_null() {
        let query = "SELECT * FROM t WHERE id IN ({IDS})";
        let params = vec![QueryParam {
            name: "IDS".to_string(),
            values: vec![],
        }];
        let (expanded, values) = expand_query_template(query, &params, postgres_placeholder);
        assert_eq!(expanded, "SELECT * FROM t WHERE id IN (NULL)");
        assert!(values.is_empty());
    }
}
