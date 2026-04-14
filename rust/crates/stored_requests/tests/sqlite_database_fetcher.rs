//! End-to-end integration tests for [`stored_requests::backends::database::DatabaseFetcher`]
//! against an on-disk SQLite database.
//!
//! SQLite `:memory:` databases under `sqlx`'s `AnyPool` are per-connection,
//! meaning a second checkout from the pool sees an empty schema. We therefore
//! use a temporary file-backed database so the `CREATE TABLE` / `INSERT`
//! performed during setup is visible to the subsequent `SELECT`s issued
//! by `DatabaseFetcher`.
//!
//! `sqlx` 0.8 requires the `Any` driver registry to be populated before the
//! first `AnyPool` connection; we call `install_default_drivers()` explicitly
//! here so the test is robust regardless of link-order behavior.

use std::collections::HashMap;

use serde_json::{json, Value};
use sqlx::any::{AnyConnectOptions, AnyPoolOptions};
use sqlx::{AnyPool, Executor};
use std::str::FromStr;
use tempfile::TempDir;

use stored_requests::backends::database::{DatabaseConfig, DatabaseFetcher};
use stored_requests::fetcher::{FetchError, Fetcher};

/// Construct a temporary on-disk SQLite database, populate it with the
/// `stored_data` fixture schema/rows, and return the pool plus the
/// tempdir guard (kept alive for the lifetime of the test).
async fn setup_pool() -> (AnyPool, TempDir) {
    // Register sqlx's compiled-in Any drivers. Calling this more than once
    // across tests is harmless.
    sqlx::any::install_default_drivers();

    let tmp = tempfile::tempdir().expect("create tempdir");
    let db_path = tmp.path().join("pbs-test.db");
    let url = format!("sqlite://{}?mode=rwc", db_path.display());

    let opts = AnyConnectOptions::from_str(&url).expect("parse sqlite url");
    let pool = AnyPoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await
        .expect("connect sqlite");

    pool.execute(
        "CREATE TABLE stored_data (
            id TEXT PRIMARY KEY,
            data TEXT NOT NULL,
            type TEXT NOT NULL
        )",
    )
    .await
    .expect("create table");

    pool.execute(
        "INSERT INTO stored_data (id, data, type) VALUES \
         ('req-1', '{\"test\":true}', 'request'), \
         ('imp-1', '{\"banner\":{}}', 'imp')",
    )
    .await
    .expect("insert rows");

    (pool, tmp)
}

fn make_fetcher(pool: AnyPool) -> DatabaseFetcher {
    DatabaseFetcher::new(DatabaseConfig {
        pool,
        query_template: "SELECT id, data, type FROM stored_data WHERE id IN ($ID_LIST)"
            .to_string(),
        amp_query_template:
            "SELECT id, data FROM stored_data WHERE type = 'response' AND id IN ($ID_LIST)"
                .to_string(),
        account_query: "SELECT data FROM stored_data WHERE type = 'account' AND id IN ($ID_LIST)"
            .to_string(),
    })
}

#[tokio::test]
async fn fetches_stored_request_from_sqlite() {
    let (pool, _tmp) = setup_pool().await;
    let fetcher = make_fetcher(pool);

    let (requests, imps, errs) = fetcher
        .fetch_requests(&["req-1".to_string()], &["imp-1".to_string()])
        .await;

    assert!(
        errs.is_empty(),
        "unexpected errors from fetch_requests: {errs:?}"
    );

    assert_eq!(requests.len(), 1, "expected 1 stored request");
    assert_eq!(imps.len(), 1, "expected 1 stored imp");

    let req = requests
        .get("req-1")
        .expect("req-1 should be in requests map");
    assert_eq!(req, &json!({"test": true}));

    let imp = imps.get("imp-1").expect("imp-1 should be in imps map");
    assert_eq!(imp, &json!({"banner": {}}));

    // Confirm the Value round-trips as JSON text as well.
    let reserialized: Value =
        serde_json::from_str(&serde_json::to_string(req).unwrap()).unwrap();
    assert_eq!(reserialized, json!({"test": true}));
}

#[tokio::test]
async fn fetch_requests_missing_ids_returns_partial() {
    let (pool, _tmp) = setup_pool().await;
    let fetcher = make_fetcher(pool);

    let (requests, imps, errs) = fetcher
        .fetch_requests(
            &["req-1".to_string(), "req-missing".to_string()],
            &[],
        )
        .await;

    assert!(imps.is_empty(), "no imps were requested");

    assert!(
        requests.contains_key("req-1"),
        "req-1 should have been fetched, got: {:?}",
        requests.keys().collect::<Vec<_>>()
    );
    assert!(
        !requests.contains_key("req-missing"),
        "req-missing should NOT be present in the map"
    );

    // The current API records a NotFound error per missing ID.
    let not_found: Vec<&FetchError> = errs
        .iter()
        .filter(|e| {
            matches!(
                e,
                FetchError::NotFound { id, data_type }
                if id == "req-missing" && data_type == "Request"
            )
        })
        .collect();
    assert_eq!(
        not_found.len(),
        1,
        "expected exactly one NotFound error for 'req-missing', got errs={errs:?}"
    );

    // Sanity: no NotFound errors for req-1.
    for e in &errs {
        if let FetchError::NotFound { id, .. } = e {
            assert_ne!(id, "req-1", "req-1 was found, should not have NotFound");
        }
    }

    // And the returned requests should only contain req-1.
    let keys: HashMap<&String, &Value> = requests.iter().collect();
    assert_eq!(keys.len(), 1);
}
