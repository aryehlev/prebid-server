//! The `Permissions` trait and its test-only implementations.
//!
//! Ported from `gdpr/gdpr.go` and `gdpr/permissions.go`. The trait is
//! `async` (via `async_trait`) because, in the full implementation, some of
//! these checks require asynchronously loading the global vendor list.

use std::collections::BTreeMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::{GdprError, Signal};

/// Per-bidder auction-time permissions.
///
/// Mirrors Go's `AuctionPermissions` struct from `gdpr/permissions.go`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuctionPermissions {
    pub allow_bid_request: bool,
    pub pass_geo: bool,
    pub pass_id: bool,
}

impl AuctionPermissions {
    pub const ALLOW_ALL: AuctionPermissions = AuctionPermissions {
        allow_bid_request: true,
        pass_geo: true,
        pass_id: true,
    };

    pub const DENY_ALL: AuctionPermissions = AuctionPermissions {
        allow_bid_request: false,
        pass_geo: false,
        pass_id: false,
    };

    pub const ALLOW_BID_REQUEST_ONLY: AuctionPermissions = AuctionPermissions {
        allow_bid_request: true,
        pass_geo: false,
        pass_id: false,
    };
}

/// Per-request context passed to the permissions builder.
///
/// Mirrors Go's `RequestInfo` struct from `gdpr/gdpr.go`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RequestInfo {
    pub alias_gvl_ids: BTreeMap<String, u16>,
    pub consent: String,
    pub gdpr_signal: Signal,
    pub publisher_id: String,
}

/// The permissions surface used by the auction pipeline.
///
/// Mirrors Go's `Permissions` interface. Methods are `async` so that
/// implementations are free to fetch remote vendor lists.
#[async_trait]
pub trait Permissions: Send + Sync {
    /// Whether the host company is allowed to read/write cookies.
    async fn host_cookies_allowed(&self) -> Result<bool, GdprError>;

    /// Whether the given bidder is allowed to perform a user sync.
    async fn bidder_sync_allowed(&self, bidder: &str) -> Result<bool, GdprError>;

    /// Full auction-time permission check for a given bidder alias.
    async fn auction_activities_allowed(
        &self,
        bidder_core_name: &str,
        bidder: &str,
    ) -> AuctionPermissions;

    /// Whether it is permissible to pass personal information (UID/Geo) to
    /// the given bidder.
    async fn personal_info_allowed(&self, bidder: &str) -> Result<bool, GdprError>;
}

/// Permissions impl that unconditionally allows everything. Useful for
/// tests and for hosts where GDPR enforcement is disabled.
#[derive(Debug, Clone, Default)]
pub struct AlwaysAllow;

#[async_trait]
impl Permissions for AlwaysAllow {
    async fn host_cookies_allowed(&self) -> Result<bool, GdprError> {
        Ok(true)
    }

    async fn bidder_sync_allowed(&self, _bidder: &str) -> Result<bool, GdprError> {
        Ok(true)
    }

    async fn auction_activities_allowed(
        &self,
        _bidder_core_name: &str,
        _bidder: &str,
    ) -> AuctionPermissions {
        AuctionPermissions::ALLOW_ALL
    }

    async fn personal_info_allowed(&self, _bidder: &str) -> Result<bool, GdprError> {
        Ok(true)
    }
}

/// Permissions impl that unconditionally denies everything. Useful for
/// tests and for fail-closed behaviour on malformed consent.
#[derive(Debug, Clone, Default)]
pub struct AlwaysFail;

#[async_trait]
impl Permissions for AlwaysFail {
    async fn host_cookies_allowed(&self) -> Result<bool, GdprError> {
        Ok(false)
    }

    async fn bidder_sync_allowed(&self, _bidder: &str) -> Result<bool, GdprError> {
        Ok(false)
    }

    async fn auction_activities_allowed(
        &self,
        _bidder_core_name: &str,
        _bidder: &str,
    ) -> AuctionPermissions {
        AuctionPermissions::DENY_ALL
    }

    async fn personal_info_allowed(&self, _bidder: &str) -> Result<bool, GdprError> {
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A minimal single-threaded async runner so this crate does not need a
    // tokio dev-dependency.
    fn block_on<F: std::future::Future>(mut f: F) -> F::Output {
        use std::pin::Pin;
        use std::sync::Arc;
        use std::task::{Context, Poll, Wake, Waker};

        struct NoopWaker;
        impl Wake for NoopWaker {
            fn wake(self: Arc<Self>) {}
        }

        let waker: Waker = Arc::new(NoopWaker).into();
        let mut cx = Context::from_waker(&waker);
        // Safety: `f` is a local variable and is not moved after this point.
        let mut f = unsafe { Pin::new_unchecked(&mut f) };
        loop {
            if let Poll::Ready(v) = f.as_mut().poll(&mut cx) {
                return v;
            }
        }
    }

    #[test]
    fn always_allow_grants_everything() {
        let p = AlwaysAllow;
        assert!(block_on(p.host_cookies_allowed()).unwrap());
        assert!(block_on(p.bidder_sync_allowed("rubicon")).unwrap());
        assert!(block_on(p.personal_info_allowed("rubicon")).unwrap());
        let ap = block_on(p.auction_activities_allowed("rubicon", "rubicon"));
        assert_eq!(ap, AuctionPermissions::ALLOW_ALL);
    }

    #[test]
    fn always_fail_denies_everything() {
        let p = AlwaysFail;
        assert!(!block_on(p.host_cookies_allowed()).unwrap());
        assert!(!block_on(p.bidder_sync_allowed("rubicon")).unwrap());
        assert!(!block_on(p.personal_info_allowed("rubicon")).unwrap());
        let ap = block_on(p.auction_activities_allowed("rubicon", "rubicon"));
        assert_eq!(ap, AuctionPermissions::DENY_ALL);
    }

    #[test]
    fn request_info_default() {
        let r = RequestInfo::default();
        assert_eq!(r.gdpr_signal, Signal::Ambiguous);
        assert!(r.consent.is_empty());
        assert!(r.publisher_id.is_empty());
        assert!(r.alias_gvl_ids.is_empty());
    }

    #[test]
    fn auction_permissions_constants() {
        assert!(AuctionPermissions::ALLOW_ALL.allow_bid_request);
        assert!(AuctionPermissions::ALLOW_ALL.pass_geo);
        assert!(AuctionPermissions::ALLOW_ALL.pass_id);

        assert!(!AuctionPermissions::DENY_ALL.allow_bid_request);
        assert!(!AuctionPermissions::DENY_ALL.pass_geo);
        assert!(!AuctionPermissions::DENY_ALL.pass_id);

        assert!(AuctionPermissions::ALLOW_BID_REQUEST_ONLY.allow_bid_request);
        assert!(!AuctionPermissions::ALLOW_BID_REQUEST_ONLY.pass_geo);
        assert!(!AuctionPermissions::ALLOW_BID_REQUEST_ONLY.pass_id);
    }
}
