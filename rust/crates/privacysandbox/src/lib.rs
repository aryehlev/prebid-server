//! Privacy Sandbox primitives - Topics API header parsing and FLEDGE /
//! interest-group types.
//!
//! Ported from the Go `privacysandbox` package.

pub mod fledge;
pub mod topics;

pub use fledge::{
    AuctionSignals, FledgeAuctionConfig, FledgeConfig, InterestGroupAuctionBuyers,
};
pub use topics::{
    parse_topics_from_header, ParseWarning, Topic, TopicsParseResult,
};
