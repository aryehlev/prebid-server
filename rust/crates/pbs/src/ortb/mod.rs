//! OpenRTB type model (Go `github.com/risecodes/openrtb` at the version pinned in `go.mod`).
//!
//! - [`openrtb2`]: every OpenRTB 2.6 object (the seller imports this package as `openrtb`).
//! - [`adcom1`]: the AdCOM code lists those objects use.
//! - [`native1`]: native 1.2 request/response markup and its code lists.
//! - [`openrtb3`]: only `NoBidReason`, used by `BidResponse.nbr`.
//!
//! # Wire compatibility with Go
//!
//! - Field order and JSON names follow the Go structs, and every Go `omitempty` is a
//!   `skip_serializing_if`. Go pointers are `Option`s, so `"dnt":0` survives a round trip.
//! - **`json.RawMessage` becomes [`Ext`]** (a `sonic_rs::Value`, key order preserved). It is
//!   written as JSON by any JSON serializer and as a msgpack `bin` of the JSON text by
//!   `rmp_serde`, which is how Go's msgpack encodes `RawMessage`. So `Imp` and `Bid` can be
//!   embedded in the bid-cache `DemandBid` msgpack blob unchanged. Use `sonic_rs` for hot-path
//!   JSON (`sonic_rs::from_slice::<BidRequest>`); `serde_json` also works.
//! - Codes (`adcom1::CreativeAttribute`, ...) are integer newtypes with named constants, not
//!   closed enums, so unknown values from buyers survive.
//! - Deserialization is lenient where Go was lenient (`null` is the zero value everywhere) and,
//!   like filtration's `flexible_types`, also takes numbers as strings and strings as numbers.
//!   See [`de`] for the exact rules. Go rejected those inputs.
//!
//! Known differences from Go's `encoding/json`:
//! - A nil non-`omitempty` slice (`imp`, `video.mimes`, `seatbid.bid`, ...) is written as `[]`,
//!   where Go writes `null`.
//! - `"ext": null` reads as `None` and is omitted when written. Go kept the literal `null`.
//! - Floats are written in Rust's shortest form (`2.0` rather than Go's `2`, `1e-7` rather than
//!   `1e-07`). The numeric value is the same.
//! - JSON keys match case-sensitively, and a duplicated known key is an error. Go matches keys
//!   case-insensitively and lets the last duplicate win.

pub mod adcom1;
pub mod de;
mod ext;
pub mod native1;
pub mod openrtb2;
pub mod openrtb3;

pub use ext::{DecodeError, Ext};

