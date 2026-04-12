//! User sync primitives for Prebid Server.
//!
//! This crate is a Rust port of the Go `usersync` package. It provides:
//!
//! - [`Cookie`] - the `uids` cookie structure and its base64+JSON codec
//! - [`Syncer`] / [`StandardSyncer`] - per-bidder sync URL generation with
//!   template macro substitution for privacy policies
//! - [`SyncType`] and [`Sync`] - response format / resolved sync descriptors
//! - [`Chooser`] / [`StandardChooser`] - selects which bidders to sync given a
//!   request and the current cookie state

pub mod chooser;
pub mod cookie;
pub mod syncer;

pub use chooser::{
    BidderEvaluation, Chooser, ChooserRequest, ChooserResult, StandardChooser, Status,
    SyncerChoice,
};
pub use cookie::{Cookie, CookieError, UidEntry, UID_COOKIE_NAME, UID_TTL_DAYS};
pub use syncer::{
    resolve_macros, PrivacyPolicies, StandardSyncer, Sync, SyncType, SyncTypeFilter, Syncer,
    SyncerError,
};
