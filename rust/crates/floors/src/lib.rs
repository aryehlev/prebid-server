//! Price floors implementation, ported from the Go `floors` package.
//!
//! This crate provides the data model and skeleton logic for price-floor
//! enrichment, enforcement, validation, and dynamic fetching of floors
//! rules. It is intentionally self-contained (no dependency on the
//! `openrtb`/`openrtb-ext` crates) so that it can be developed in
//! isolation.

pub mod enforce;
pub mod fetcher;
pub mod rule;
pub mod types;
pub mod validate;

pub use enforce::{enforce_floors_rules, EnforceOutcome, RejectedBid, SeatBid};
pub use fetcher::{
    fetch_price_floor_rules, fetch_price_floor_rules_with, FetchError, FetchResult, FetchStatus,
    FetchStatusCode, FetchedFloors, FloorFetcher, FloorFetcherConfig,
};
pub use rule::{create_rule_key, find_rule, round_to_four_decimals};
pub use types::{
    PriceFloorData, PriceFloorEnforcement, PriceFloorEndpoint, PriceFloorModelGroup,
    PriceFloorRules, PriceFloorSchema, PriceFloors, SchemaDimension,
};
pub use validate::{validate, validate_floor_params, FloorsError};
