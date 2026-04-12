//! # experimental-fuzz
//!
//! **EXPERIMENTAL** crate providing property-based testing and mutation
//! fuzzers targeting the Prebid request/response surface.
//!
//! This crate is purpose-built to uncover edge cases in request parsing and
//! validation. It offers:
//!
//! - [`generators`] — structured generators (via `arbitrary`) that build
//!   minimally valid OpenRTB-shaped JSON requests.
//! - [`mutators`] — trait-based mutation primitives (drop field, negate
//!   number, swap types, truncate arrays, chain).
//! - [`strategies`] — `proptest::Strategy` builders for valid `imp`,
//!   `banner`, `site`, `user` objects, and a composed `bid_request_strategy`.
//! - [`oracle`] — differential oracles that compare original vs mutated
//!   values for equivalence or expected failure.
//! - [`error`] — shared [`FuzzError`] type.
//!
//! This crate is isolated from the rest of the workspace (no internal
//! dependencies) so it can be iterated on freely without affecting
//! production code.

pub mod error;
pub mod generators;
pub mod mutators;
pub mod oracle;
pub mod strategies;

pub use error::FuzzError;
pub use generators::{bidder_pool, generate_imp, generate_request, ArbRequest};
pub use mutators::{ChainMutator, DropField, Mutator, NegateNumber, SwapStringType, TruncateArray};
pub use oracle::{FieldRemovalOracle, Oracle, OracleResult, SerializationOracle};
pub use strategies::bid_request_strategy;
