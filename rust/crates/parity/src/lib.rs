//! Parity crate: ports the Go `parity` package used to verify response parity
//! between two implementations (Go vs Rust) for regression testing during the
//! migration.
//!
//! Core building blocks:
//! - [`diff`] — a structural JSON diff engine with tolerance and ignore support.
//! - [`normalize`] — in-place JSON normalization (sort arrays, strip ignored paths).
//! - [`options`] — knobs that drive both diffing and normalization.
//! - [`proxy`] — a shadow proxy that forwards a request to two backends in
//!   parallel and emits diffs between their responses.
//! - [`reporter`] — pluggable sinks for diff entries (tracing, file).
//! - [`error`] — crate-wide error enum.

pub mod diff;
pub mod error;
pub mod normalize;
pub mod options;
pub mod proxy;
pub mod reporter;

pub use diff::{diff, DiffEntry, DiffKind, JsonDiff};
pub use error::ParityError;
pub use normalize::normalize;
pub use options::DiffOptions;
pub use proxy::{ProxyError, ShadowProxy, ShadowResponse};
pub use reporter::{DiffReporter, FileReporter, TracingReporter};
