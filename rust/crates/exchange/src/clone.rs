//! Deep cloning utilities for OpenRTB structures.
//! Mirrors Go `ortb/clone.go`.

use openrtb::*;

/// Deep clone a User, including nested Geo, Data, EIDs.
pub fn clone_user(user: &User) -> User {
    user.clone()  // Rust's Clone already does deep copy for owned types
}

/// Deep clone a Device, including nested Geo.
pub fn clone_device(device: &Device) -> Device {
    device.clone()
}

/// Deep clone a Source, including SChain.
pub fn clone_source(source: &Source) -> Source {
    source.clone()
}

/// Deep clone a Regs object.
pub fn clone_regs(regs: &Regs) -> Regs {
    regs.clone()
}

/// Partial clone of BidRequest - only clones Device, User, and Source.
/// Other fields are shallow-copied (shared references for Imp list, etc.).
pub fn clone_bid_request_partial(req: &BidRequest) -> BidRequest {
    let mut c = req.clone();
    if let Some(ref device) = req.device {
        c.device = Some(clone_device(device));
    }
    if let Some(ref user) = req.user {
        c.user = Some(clone_user(user));
    }
    if let Some(ref source) = req.source {
        c.source = Some(clone_source(source));
    }
    c
}

/// Deep clone a Geo object.
pub fn clone_geo(geo: &Geo) -> Geo {
    geo.clone()
}
