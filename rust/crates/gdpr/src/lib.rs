//! GDPR / TCF2 primitives ported from the Go `gdpr` package.
//!
//! This crate provides the core permissions model, signal handling,
//! consent-string wrapper and basic TCF2 vendor-list data types used by
//! prebid-server. It is a standalone crate and depends only on the workspace
//! dependencies listed in `Cargo.toml`.

pub mod consent;
pub mod permissions;
pub mod signal;
pub mod vendorlist;

pub use consent::ConsentString;
pub use permissions::{
    AlwaysAllow, AlwaysFail, AuctionPermissions, Permissions, RequestInfo,
};
pub use signal::{Signal, SignalError};
pub use vendorlist::{Purpose, PurposeId, Vendor, VendorId, VendorList};

use thiserror::Error;

/// Error returned when the consent string argument was the reason for the
/// failure. Mirrors Go's `ErrorMalformedConsent`.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error("malformed consent string {consent}: {cause}")]
pub struct MalformedConsentError {
    pub consent: String,
    pub cause: String,
}

/// Top-level error type for the GDPR crate.
#[derive(Debug, Error)]
pub enum GdprError {
    #[error(transparent)]
    MalformedConsent(#[from] MalformedConsentError),

    #[error("invalid GDPR signal: {0}")]
    Signal(#[from] SignalError),

    #[error("vendor list not found: spec={spec_version} list={list_version}")]
    VendorListNotFound {
        spec_version: u16,
        list_version: u16,
    },

    #[error("{0}")]
    Other(String),
}
