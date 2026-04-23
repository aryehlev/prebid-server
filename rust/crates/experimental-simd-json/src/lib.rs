//! # experimental-simd-json
//!
//! **EXPERIMENTAL.** This crate is a proof-of-concept for replacing the
//! `serde_json`-based OpenRTB request parser with a `simd-json` fast path.
//! It is **not** wired into the production request pipeline and its public
//! API may change or disappear entirely without notice.
//!
//! The crate purposefully has no dependencies on other internal Prebid
//! Server crates so it can be swapped out or deleted without ripple
//! effects. See the `parser` module for the fast extraction helpers and
//! `serde_bridge` for interoperating with code that still uses
//! `serde_json::Value`.

pub mod bench_support;
pub mod error;
pub mod parser;
pub mod serde_bridge;

pub use error::ParseError;
pub use parser::*;
pub use serde_bridge::*;
