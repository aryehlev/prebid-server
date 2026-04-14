//! `e2e-tests` — YAML-driven end-to-end auction test harness for the Rust
//! Prebid Server port.
//!
//! The crate lets you describe an auction scenario as a YAML file containing:
//!
//! * a [`Scenario`] request / expected response pair,
//! * in-memory "setup" data (stored requests, stored imps, accounts, and
//!   pre-canned bid responses),
//! * a list of expected warnings.
//!
//! A [`ScenarioRunner`] wires the setup into an in-memory
//! [`stored_requests::Fetcher`], invokes an [`AuctionBackend`] implementation
//! against the request JSON, and diffs the result against the expected
//! response using [`parity::diff::diff`] with a configurable ignore list.
//!
//! For unit tests and CI bring-up a [`backend::StubAuctionBackend`] is
//! provided: it simply echoes the `mock_response` embedded in the request (or
//! returns an empty `BidResponse` if absent), which makes the harness useful
//! even before the real auction pipeline is hooked up.
//!
//! A [`TestSuite`] discovers every `*.yaml` file in a directory and runs them
//! sequentially, reporting aggregate pass/fail counts.

pub mod backend;
pub mod error;
pub mod fixtures;
pub mod runner;
pub mod scenario;
pub mod suite;

pub use backend::{AuctionBackend, BackendError, StubAuctionBackend};
pub use error::E2eError;
pub use runner::{ScenarioResult, ScenarioRunner};
pub use scenario::{Scenario, ScenarioSetup};
pub use suite::{SuiteResult, TestSuite};
