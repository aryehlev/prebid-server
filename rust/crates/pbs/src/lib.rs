//! Prebid Server (Go v3.30.0) bidder adapters, ported for the seller.
//!
//! Adapters take the typed [`ortb::openrtb2::BidRequest`] directly: there is no
//! marshal/unmarshal between the seller's ORTB model and a second Prebid model.

pub mod adapters;
pub mod bid_types;
pub mod bidder_info;
pub mod bidder;
pub mod config;
pub mod currency;
pub mod errortypes;
pub mod ext_helpers;
pub mod go_json;
pub mod header;
pub mod jsonutil;
pub mod macros;
pub mod ortb;
pub mod registry;
pub mod testing;
