//! Loggable analytics event types.
//!
//! These mirror the Go `analytics` package event structs. Each event carries a
//! timestamp, request id, account id, HTTP-like status code, and a
//! `serde_json::Value` holding the full request/response payload so the shape
//! can evolve independently of this crate.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Discriminator describing which endpoint an event originated from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    Auction,
    Amp,
    Video,
    SetUid,
    CookieSync,
    Notification,
}

macro_rules! define_event {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Serialize, Deserialize)]
        pub struct $name {
            /// Timestamp at which the event was emitted.
            pub timestamp: DateTime<Utc>,
            /// Identifier of the originating request.
            pub request_id: String,
            /// Identifier of the associated publisher account, if any.
            pub account_id: String,
            /// HTTP-like status code summarising the outcome.
            pub status: u16,
            /// Full request/response payload, opaquely captured as JSON so
            /// upstream schema changes do not break this crate.
            pub payload: Value,
        }

        impl $name {
            /// Construct a new event with the current UTC timestamp.
            pub fn new(
                request_id: impl Into<String>,
                account_id: impl Into<String>,
                status: u16,
                payload: Value,
            ) -> Self {
                Self {
                    timestamp: Utc::now(),
                    request_id: request_id.into(),
                    account_id: account_id.into(),
                    status,
                    payload,
                }
            }
        }
    };
}

define_event!(
    /// Loggable object for an `/openrtb2/auction` transaction.
    AuctionObject
);
define_event!(
    /// Loggable object for an `/openrtb2/amp` transaction.
    AmpObject
);
define_event!(
    /// Loggable object for an `/openrtb2/video` transaction.
    VideoObject
);
define_event!(
    /// Loggable object for a `/setuid` transaction.
    SetUidObject
);
define_event!(
    /// Loggable object for a `/cookie_sync` transaction.
    CookieSyncObject
);
define_event!(
    /// Loggable object for an `/event` notification.
    NotificationEvent
);

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn auction_object_round_trips() {
        let evt = AuctionObject::new("req-1", "acct-1", 200, json!({"hello": "world"}));
        let s = serde_json::to_string(&evt).expect("serialize");
        let back: AuctionObject = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(back.request_id, "req-1");
        assert_eq!(back.account_id, "acct-1");
        assert_eq!(back.status, 200);
        assert_eq!(back.payload, json!({"hello": "world"}));
    }

    #[test]
    fn event_type_round_trips() {
        let s = serde_json::to_string(&EventType::CookieSync).unwrap();
        assert_eq!(s, "\"cookie_sync\"");
        let back: EventType = serde_json::from_str(&s).unwrap();
        assert_eq!(back, EventType::CookieSync);
    }

    #[test]
    fn all_event_types_serialize() {
        let payload = json!({"k": 1});
        let _a = serde_json::to_string(&AmpObject::new("r", "a", 200, payload.clone())).unwrap();
        let _v = serde_json::to_string(&VideoObject::new("r", "a", 200, payload.clone())).unwrap();
        let _s = serde_json::to_string(&SetUidObject::new("r", "a", 200, payload.clone())).unwrap();
        let _c =
            serde_json::to_string(&CookieSyncObject::new("r", "a", 200, payload.clone())).unwrap();
        let _n =
            serde_json::to_string(&NotificationEvent::new("r", "a", 200, payload.clone())).unwrap();
    }
}
