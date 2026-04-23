//! Database-backed fetcher using `sqlx`'s `AnyPool`.
//!
//! The configured query is expected to return rows of the shape
//! `(id TEXT, data JSON/TEXT, type TEXT)` where `type` is `"request"`,
//! `"imp"`, or `"response"`.
//!
//! `query_template` may contain the following placeholders, substituted
//! at query time:
//!
//! * `$LAST_UPDATED` — replaced with the literal string `NULL` (callers
//!   can override by providing a concrete timestamp in their template).
//! * `$ID_LIST` — replaced with a comma-separated list of single-quoted IDs.

use async_trait::async_trait;
use serde_json::Value;
use sqlx::any::{AnyConnectOptions, AnyPoolOptions};
use sqlx::{AnyPool, Row};
use std::collections::HashMap;
use std::str::FromStr;

use crate::fetcher::{FetchError, Fetcher};

/// Configuration for a [`DatabaseFetcher`].
pub struct DatabaseConfig {
    /// An `sqlx` connection pool over the `Any` database backend.
    pub pool: AnyPool,
    /// Query template for stored requests / imps.
    ///
    /// Must contain `$ID_LIST` and may contain `$LAST_UPDATED`.
    pub query_template: String,
    /// Query template used for AMP stored requests.
    pub amp_query_template: String,
    /// Query used for fetching account configuration by ID.
    ///
    /// Should produce a single row with the JSON payload in column 0.
    pub account_query: String,
}

/// Database-backed [`Fetcher`].
pub struct DatabaseFetcher {
    /// The fetcher's configuration, including the connection pool.
    pub config: DatabaseConfig,
}

impl DatabaseFetcher {
    /// Create a new [`DatabaseFetcher`] from an already-constructed
    /// [`DatabaseConfig`].
    pub fn new(config: DatabaseConfig) -> Self {
        Self { config }
    }

    /// Build a new [`DatabaseFetcher`] by connecting to the given URL.
    ///
    /// This is a convenience wrapper around
    /// [`AnyConnectOptions::from_str`] + [`AnyPoolOptions`].
    pub async fn connect(
        url: &str,
        query_template: impl Into<String>,
        amp_query_template: impl Into<String>,
        account_query: impl Into<String>,
    ) -> Result<Self, FetchError> {
        let opts = AnyConnectOptions::from_str(url)
            .map_err(|e| FetchError::Database(e.to_string()))?;
        let pool = AnyPoolOptions::new()
            .connect_with(opts)
            .await
            .map_err(|e| FetchError::Database(e.to_string()))?;
        Ok(Self::new(DatabaseConfig {
            pool,
            query_template: query_template.into(),
            amp_query_template: amp_query_template.into(),
            account_query: account_query.into(),
        }))
    }
}

/// Substitute `$ID_LIST` / `$LAST_UPDATED` placeholders in a query template.
///
/// IDs are single-quoted and comma-separated. Embedded single quotes in
/// IDs are doubled to mitigate trivial injection attempts; callers are
/// still responsible for validating IDs upstream.
fn substitute(template: &str, ids: &[String]) -> String {
    let joined = if ids.is_empty() {
        "NULL".to_string()
    } else {
        ids.iter()
            .map(|id| format!("'{}'", id.replace('\'', "''")))
            .collect::<Vec<_>>()
            .join(",")
    };
    template
        .replace("$ID_LIST", &joined)
        .replace("$LAST_UPDATED", "NULL")
}

#[async_trait]
impl Fetcher for DatabaseFetcher {
    async fn fetch_requests(
        &self,
        req_ids: &[String],
        imp_ids: &[String],
    ) -> (
        HashMap<String, Value>,
        HashMap<String, Value>,
        Vec<FetchError>,
    ) {
        if req_ids.is_empty() && imp_ids.is_empty() {
            return (HashMap::new(), HashMap::new(), Vec::new());
        }

        // Combine req + imp IDs into a single $ID_LIST substitution. The
        // query is expected to discriminate via the `type` column.
        let mut combined: Vec<String> = Vec::with_capacity(req_ids.len() + imp_ids.len());
        combined.extend_from_slice(req_ids);
        combined.extend_from_slice(imp_ids);

        let sql = substitute(&self.config.query_template, &combined);

        let rows = match sqlx::query(&sql).fetch_all(&self.config.pool).await {
            Ok(r) => r,
            Err(e) => {
                return (
                    HashMap::new(),
                    HashMap::new(),
                    vec![FetchError::Database(e.to_string())],
                );
            }
        };

        let mut requests: HashMap<String, Value> = HashMap::with_capacity(req_ids.len());
        let mut imps: HashMap<String, Value> = HashMap::with_capacity(imp_ids.len());
        let mut errs = Vec::new();

        for row in rows {
            let id: String = match row.try_get::<String, _>(0) {
                Ok(v) => v,
                Err(e) => {
                    errs.push(FetchError::Database(e.to_string()));
                    continue;
                }
            };
            let data_str: String = match row.try_get::<String, _>(1) {
                Ok(v) => v,
                Err(e) => {
                    errs.push(FetchError::Database(e.to_string()));
                    continue;
                }
            };
            let data_type: String = match row.try_get::<String, _>(2) {
                Ok(v) => v,
                Err(e) => {
                    errs.push(FetchError::Database(e.to_string()));
                    continue;
                }
            };
            let value: Value = match serde_json::from_str(&data_str) {
                Ok(v) => v,
                Err(e) => {
                    errs.push(FetchError::from(e));
                    continue;
                }
            };

            match data_type.as_str() {
                "request" => {
                    requests.insert(id, value);
                }
                "imp" => {
                    imps.insert(id, value);
                }
                other => {
                    tracing::debug!(id = %id, data_type = %other,
                        "Database result with unknown type — ignoring");
                }
            }
        }

        for id in req_ids {
            if !requests.contains_key(id) {
                errs.push(FetchError::not_found(id.clone(), "Request"));
            }
        }
        for id in imp_ids {
            if !imps.contains_key(id) {
                errs.push(FetchError::not_found(id.clone(), "Imp"));
            }
        }

        (requests, imps, errs)
    }

    async fn fetch_account(&self, account_id: &str) -> Result<Value, FetchError> {
        if account_id.is_empty() {
            return Err(FetchError::Other("Cannot look up an empty accountID".into()));
        }
        let sql = substitute(&self.config.account_query, &[account_id.to_string()]);
        let row_opt = sqlx::query(&sql)
            .fetch_optional(&self.config.pool)
            .await
            .map_err(|e| FetchError::Database(e.to_string()))?;

        let row = row_opt.ok_or_else(|| FetchError::not_found(account_id, "Account"))?;
        let data_str: String = row
            .try_get::<String, _>(0)
            .map_err(|e| FetchError::Database(e.to_string()))?;
        Ok(serde_json::from_str(&data_str)?)
    }

    async fn fetch_categories(
        &self,
        primary_adserver: &str,
        publisher_id: &str,
    ) -> Result<String, FetchError> {
        Err(FetchError::Other(format!(
            "DatabaseFetcher does not support fetch_categories (adserver={primary_adserver}, publisher={publisher_id})"
        )))
    }

    async fn fetch_responses(
        &self,
        ids: &[String],
    ) -> Result<HashMap<String, Value>, FetchError> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let sql = substitute(&self.config.amp_query_template, ids);
        let rows = sqlx::query(&sql)
            .fetch_all(&self.config.pool)
            .await
            .map_err(|e| FetchError::Database(e.to_string()))?;

        let mut out: HashMap<String, Value> = HashMap::with_capacity(ids.len());
        for row in rows {
            let id: String = row
                .try_get::<String, _>(0)
                .map_err(|e| FetchError::Database(e.to_string()))?;
            let data_str: String = row
                .try_get::<String, _>(1)
                .map_err(|e| FetchError::Database(e.to_string()))?;
            let value: Value = serde_json::from_str(&data_str)?;
            out.insert(id, value);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::substitute;

    #[test]
    fn substitute_id_list_and_last_updated() {
        let tmpl = "SELECT id, data, type FROM t WHERE id IN ($ID_LIST) AND updated > $LAST_UPDATED";
        let ids = vec!["a".to_string(), "b".to_string()];
        let sql = substitute(tmpl, &ids);
        assert_eq!(
            sql,
            "SELECT id, data, type FROM t WHERE id IN ('a','b') AND updated > NULL"
        );
    }

    #[test]
    fn substitute_escapes_single_quotes() {
        let tmpl = "IN ($ID_LIST)";
        let ids = vec!["o'reilly".to_string()];
        let sql = substitute(tmpl, &ids);
        assert_eq!(sql, "IN ('o''reilly')");
    }

    #[test]
    fn substitute_empty_ids() {
        let tmpl = "IN ($ID_LIST)";
        let sql = substitute(tmpl, &[]);
        assert_eq!(sql, "IN (NULL)");
    }

    #[cfg(feature = "integration-tests")]
    mod integration {
        // Real DB integration tests live here behind the `integration-tests`
        // feature flag so default `cargo test` never tries to connect.
    }
}
