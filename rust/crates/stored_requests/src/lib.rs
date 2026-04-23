//! `stored_requests` — Rust port of the Go `stored_requests` package.
//!
//! Provides the [`Fetcher`] trait and a set of concrete backends for loading
//! stored request / stored imp / stored response / account JSON by ID:
//!
//! * [`backends::filesystem::FileFetcher`] — eagerly loads JSON from a
//!   directory tree at startup.
//! * [`backends::http::HttpFetcher`] — queries a remote HTTP endpoint.
//! * [`backends::database::DatabaseFetcher`] — queries a SQL database via
//!   `sqlx`'s `AnyPool`.
//!
//! A [`cache::CachedFetcher`] wrapper is also provided that can front any
//! inner [`Fetcher`] with in-process `moka` caches for requests and imps.

pub mod backends;
pub mod cache;
pub mod fetcher;

pub use cache::CachedFetcher;
pub use fetcher::{FetchError, Fetcher};
