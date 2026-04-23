//! Rust port of the Prebid Server Go `experiment` package.
//!
//! Upstream, the Go package is mostly concerned with adscert signing
//! (`experiment/adscert`). The configuration types for the experimental
//! features live in `config/experiment.go`. This crate re-implements the
//! essential configuration types, plus a small A/B bucketing utility that
//! is commonly layered on top of experimental feature flags: a
//! deterministic hashing-based splitter that maps an account id or request
//! id into an experiment bucket.

pub mod adscert;
pub mod adscert_inprocess;
pub mod adscert_remote;
pub mod ab;
pub mod config;

pub use adscert::{AdCertsSignerMode, AdsCertInProcess, AdsCertRemote, ExperimentAdsCert};
pub use ab::{bucket_for, select_variant, ExperimentConfig, Variant};
pub use config::{Experiment, ExperimentValidationError};
