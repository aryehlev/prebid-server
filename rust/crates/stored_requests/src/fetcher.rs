//! Core [`Fetcher`] trait and [`FetchError`] error type.

use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use thiserror::Error;

/// Errors that can be produced by a [`Fetcher`] implementation.
#[derive(Debug, Error)]
pub enum FetchError {
    /// A specific ID was requested but not found by the backend.
    #[error("Stored {data_type} with ID=\"{id}\" not found.")]
    NotFound {
        /// The ID that was requested.
        id: String,
        /// A human-readable data type label such as `"Request"`, `"Imp"`,
        /// `"Response"`, or `"Account"`.
        data_type: String,
    },

    /// An error occurred while performing I/O (filesystem, network, etc.).
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// An error occurred while parsing JSON.
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),

    /// An error occurred while issuing an HTTP request.
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// An error occurred while querying the database.
    #[error("Database error: {0}")]
    Database(String),

    /// A backend-specific error with a free-form message.
    #[error("{0}")]
    Other(String),
}

impl FetchError {
    /// Convenience constructor for [`FetchError::NotFound`].
    pub fn not_found(id: impl Into<String>, data_type: impl Into<String>) -> Self {
        FetchError::NotFound {
            id: id.into(),
            data_type: data_type.into(),
        }
    }
}

/// A `Fetcher` knows how to load stored request data by ID.
///
/// Implementations must be safe to share across tasks.
#[async_trait]
pub trait Fetcher: Send + Sync {
    /// Fetch stored requests and stored imps by their IDs.
    ///
    /// Returns two maps (requests, imps) plus any accumulated errors, one
    /// per missing ID.
    async fn fetch_requests(
        &self,
        req_ids: &[String],
        imp_ids: &[String],
    ) -> (
        HashMap<String, Value>,
        HashMap<String, Value>,
        Vec<FetchError>,
    );

    /// Fetch a single account configuration by ID.
    async fn fetch_account(&self, account_id: &str) -> Result<Value, FetchError>;

    /// Fetch a category mapping string for the given ad server and publisher.
    async fn fetch_categories(
        &self,
        primary_adserver: &str,
        publisher_id: &str,
    ) -> Result<String, FetchError>;

    /// Fetch stored responses by their IDs.
    async fn fetch_responses(
        &self,
        ids: &[String],
    ) -> Result<HashMap<String, Value>, FetchError>;
}
