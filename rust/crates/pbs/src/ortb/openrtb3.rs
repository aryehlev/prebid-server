//! The one OpenRTB 3 list the seller uses (Go `openrtb3.NoBidReason`, `bidresponse.nbr`).

use super::de::ortb_code;

ortb_code! {
    /// List: No-Bid Reason Codes.
    NoBidReason(i64) {
        UNKNOWN_ERROR = 0,
        TECHNICAL_ERROR = 1,
        INVALID_REQUEST = 2,
        CRAWLER = 3,
        NON_HUMAN = 4,
        PROXY = 5,
        UNSUPPORTED_DEVICE = 6,
        BLOCKED_PUBLISHER = 7,
        UNMATCHED_USER = 8,
        DAILY_USER_CAP = 9,
        DAILY_DOMAIN_CAP = 10,
        AUTHORIZATION_UNAVAILABLE = 11,
        AUTHORIZATION_VIOLATION = 12,
        AUTHENTICATION_UNAVAILABLE = 13,
        AUTHENTICATION_VIOLATION = 14,
        INSUFFICIENT_TIME = 15,
        INCOMPLETE_SUPPLY_CHAIN = 16,
        BLOCKED_SUPPLY_CHAIN_NODE = 17,
    }
}
