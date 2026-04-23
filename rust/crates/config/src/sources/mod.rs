//! Pluggable configuration sources.
//!
//! These are secondary to [`crate::loader::ConfigLoader`]: they provide
//! *runtime* configuration refresh semantics for things like feature flags
//! or bidder info blobs that change after startup.

pub mod remote;

pub use remote::{RemoteConfigSource, RemoteSourceError};
