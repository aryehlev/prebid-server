//! OpenRTB 2.6 objects (Go `github.com/risecodes/openrtb/openrtb2`, imported as `openrtb` in the seller).
//!
//! Field order follows the Go structs, so serialized key order matches Go's `json.Marshal`.
//! Field names are the JSON wire names (`bidfloorcur`, `seatbid`, ...); `type`/`ref` become
//! `r#type`/`r#ref`. Acronym type names use Rust casing: `PMP` is [`Pmp`], `DOOH` is [`Dooh`],
//! `EID`/`UID` are [`Eid`]/[`Uid`].
//!
//! Go pointers to scalars (`*int8`, `*adcom1.StartDelay`, ...) are `Option`s; Go pointers to
//! structs are `Option<T>`. Go's `Int64Ptr`/`Int8Ptr` helpers are just `Some(x)`.

mod context;
mod device;
mod imp;
mod request;
mod response;

pub use context::{App, Channel, Content, Data, Dooh, Network, Producer, Publisher, Segment, Site};
pub use device::{BrandVersion, Device, Eid, Geo, Uid, User, UserAgent};
pub use imp::{
    Audio, Banner, Deal, Format, Imp, Metric, Native, Pmp, Qty, RefSettings, Refresh, Video,
};
pub use request::{BidRequest, Regs, Source, SupplyChain, SupplyChainNode};
pub use response::{Bid, BidResponse, SeatBid};

use super::de::ortb_code;

ortb_code! {
    /// OpenRTB 2.6 List: Ad Insertion (`imp.ssai`).
    AdInsertion(i8) {
        UNKNOWN = 0,
        CLIENT = 1,
        SERVER_STITCH_CLIENT_TRACK = 2,
        SERVER = 3,
    }
}

ortb_code! {
    /// OpenRTB 2.5 List: Banner Ad Types (`banner.btype`).
    BannerAdType(i8) {
        XHTML_TEXT_AD = 1,
        XHTML_BANNER_AD = 2,
        JAVASCRIPT_AD = 3,
        IFRAME = 4,
    }
}

ortb_code! {
    /// Bid markup type (`bid.mtype`).
    MarkupType(i8) {
        BANNER = 1,
        VIDEO = 2,
        AUDIO = 3,
        NATIVE = 4,
    }
}
