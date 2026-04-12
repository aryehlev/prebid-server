//! # experimental-quic
//!
//! **EXPERIMENTAL** HTTP/3 (QUIC) auction endpoint for Prebid Server.
//!
//! Prebid Server's canonical `/openrtb2/auction` endpoint is served over
//! HTTP/1.1 (see `crates/server` and `crates/endpoints`). This crate is an
//! exploration of serving the same endpoint over HTTP/3 using
//! [`quinn`](https://docs.rs/quinn) for the QUIC transport and
//! [`h3`](https://docs.rs/h3) for the HTTP/3 protocol layer.
//!
//! The goals of the experiment are:
//!
//! - Measure whether the 0-RTT / multiplexing properties of HTTP/3 offer
//!   meaningful latency wins for auction traffic.
//! - Provide a self-contained harness with self-signed certificates for
//!   integration testing.
//! - Keep the implementation isolated from the rest of the workspace – this
//!   crate depends on no other internal crates.
//!
//! The public surface is intentionally small:
//!
//! - [`server::QuicAuctionServer`] – binds a QUIC endpoint and dispatches
//!   POST `/openrtb2/auction` requests to a handler.
//! - [`handler::AuctionHandler`] – user-supplied async trait that turns a
//!   request body into a response body.
//! - [`handler::StubAuctionHandler`] – echo implementation used in tests.
//! - [`tls`] – helpers for loading PEM certs and generating self-signed DER
//!   material via `rcgen`.
//! - [`client::QuicAuctionClient`] – a minimal HTTP/3 client used by the
//!   integration tests.
//!
//! This crate is **not** wired into the production workspace members list.

pub mod client;
pub mod error;
pub mod handler;
pub mod server;
pub mod tls;

pub use error::QuicError;
pub use handler::{AuctionHandler, HandlerError, StubAuctionHandler};
pub use server::{QuicAuctionServer, QuicServerConfig};
