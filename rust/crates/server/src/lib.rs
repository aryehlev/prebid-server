//! HTTP server skeleton for Prebid Server (Rust port).
//!
//! This crate is intentionally kept self-contained (no dependencies on other
//! internal workspace crates) so that it can build independently while the
//! rest of the Rust port is still in flight.
//!
//! The public API exposes three modules:
//! * [`config`] – server configuration loaded from environment variables.
//! * [`router`] – constructs the [`axum::Router`] with placeholder routes and
//!   middleware stack.
//! * [`lifecycle`] – graceful shutdown helpers (Ctrl-C / SIGTERM).

pub mod config;
pub mod lifecycle;
pub mod router;
