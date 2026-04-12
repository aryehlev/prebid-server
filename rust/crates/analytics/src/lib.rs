//! Analytics crate for Prebid Server (Rust port).
//!
//! This crate provides a pluggable analytics module system, mirroring the Go
//! `analytics` package. Consumers construct an [`AnalyticsRunner`] wrapping a
//! collection of [`PbsAnalyticsModule`] implementations and call the various
//! `log_*` methods to fan events out to every registered backend.

pub mod backends;
pub mod events;
pub mod module;
pub mod runner;

pub use events::{
    AmpObject, AuctionObject, CookieSyncObject, EventType, NotificationEvent, SetUidObject,
    VideoObject,
};
pub use module::PbsAnalyticsModule;
pub use runner::AnalyticsRunner;

pub use backends::file::FileAnalyticsBackend;
pub use backends::null::NullAnalyticsBackend;
pub use backends::pubstack::PubstackAnalyticsBackend;
pub use backends::stdout::StdoutAnalyticsBackend;
