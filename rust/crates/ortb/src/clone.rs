//! Deep cloning functions for OpenRTB types.
//!
//! Mirrors Go `ortb/clone.go`. In Rust, most types already derive Clone,
//! but these functions provide the exact same semantics as the Go code:
//! deep cloning of nested structures to avoid shared mutable state.

use openrtb::*;

/// Clone a BidRequest partially — only Device, User, and Source are deep-cloned.
/// Other fields are shallow-copied (they share references to the same data).
/// Mirrors Go `CloneBidRequestPartial`.
pub fn clone_bid_request_partial(req: &BidRequest) -> BidRequest {
    let mut c = req.clone();
    c.device = req.device.as_ref().map(|d| clone_device(d));
    c.user = req.user.as_ref().map(|u| clone_user(u));
    c.source = req.source.as_ref().map(|s| clone_source(s));
    c
}

/// Deep clone a User.
/// Mirrors Go `CloneUser`.
pub fn clone_user(u: &User) -> User {
    let mut c = u.clone();
    c.geo = u.geo.as_ref().map(|g| clone_geo(g));
    c.data = clone_data_slice(&u.data);
    c.eids = u.eids.as_ref().map(|e| clone_eid_slice(e));
    c.ext = u.ext.clone();
    c
}

/// Deep clone a Device.
/// Mirrors Go `CloneDevice`.
pub fn clone_device(d: &Device) -> Device {
    let mut c = d.clone();
    c.geo = d.geo.as_ref().map(|g| clone_geo(g));
    c.sua = d.sua.as_ref().map(|s| clone_user_agent(s));
    c.ext = d.ext.clone();
    c
}

/// Deep clone a UserAgent (SUA).
/// Mirrors Go `CloneUserAgent`.
pub fn clone_user_agent(s: &UserAgent) -> UserAgent {
    let mut c = s.clone();
    c.browsers = s.browsers.as_ref().map(|b| clone_brand_version_slice(b));
    c.platform = s.platform.as_ref().map(|p| clone_brand_version(p));
    c.ext = s.ext.clone();
    c
}

/// Deep clone a slice of BrandVersion.
fn clone_brand_version_slice(s: &[BrandVersion]) -> Vec<BrandVersion> {
    s.iter().map(|bv| clone_brand_version(bv)).collect()
}

/// Deep clone a BrandVersion.
fn clone_brand_version(bv: &BrandVersion) -> BrandVersion {
    let mut c = bv.clone();
    c.version = bv.version.clone();
    c.ext = bv.ext.clone();
    c
}

/// Deep clone a Source.
/// Mirrors Go `CloneSource`.
pub fn clone_source(s: &Source) -> Source {
    let mut c = s.clone();
    c.schain = s.schain.as_ref().map(|sc| clone_schain(sc));
    c.ext = s.ext.clone();
    c
}

/// Deep clone a SupplyChain.
/// Mirrors Go `CloneSChain`.
pub fn clone_schain(sc: &SupplyChain) -> SupplyChain {
    let mut c = sc.clone();
    c.nodes = clone_supply_chain_nodes(&sc.nodes);
    c.ext = sc.ext.clone();
    c
}

/// Deep clone a slice of SupplyChainNode.
fn clone_supply_chain_nodes(nodes: &[SupplyChainNode]) -> Vec<SupplyChainNode> {
    nodes.iter().map(|n| clone_supply_chain_node(n)).collect()
}

/// Deep clone a SupplyChainNode.
fn clone_supply_chain_node(n: &SupplyChainNode) -> SupplyChainNode {
    let mut c = n.clone();
    c.ext = n.ext.clone();
    c
}

/// Deep clone a Geo.
/// Mirrors Go `CloneGeo`.
pub fn clone_geo(g: &Geo) -> Geo {
    let mut c = g.clone();
    c.ext = g.ext.clone();
    c
}

/// Deep clone a Regs.
/// Mirrors Go `CloneRegs`.
pub fn clone_regs(r: &Regs) -> Regs {
    let mut c = r.clone();
    c.ext = r.ext.clone();
    c
}

/// Deep clone a slice of Data.
fn clone_data_slice(data: &[Data]) -> Vec<Data> {
    data.iter().map(|d| clone_data(d)).collect()
}

/// Deep clone a Data.
fn clone_data(d: &Data) -> Data {
    let mut c = d.clone();
    c.segment = clone_segment_slice(&d.segment);
    c.ext = d.ext.clone();
    c
}

/// Deep clone a slice of Segment.
fn clone_segment_slice(segments: &[Segment]) -> Vec<Segment> {
    segments.iter().map(|s| clone_segment(s)).collect()
}

/// Deep clone a Segment.
fn clone_segment(s: &Segment) -> Segment {
    let mut c = s.clone();
    c.ext = s.ext.clone();
    c
}

/// Deep clone a slice of EID.
fn clone_eid_slice(eids: &[Eid]) -> Vec<Eid> {
    eids.iter().map(|e| clone_eid(e)).collect()
}

/// Deep clone an EID.
fn clone_eid(e: &Eid) -> Eid {
    let mut c = e.clone();
    c.uids = e.uids.as_ref().map(|u| clone_uid_slice(u));
    c.ext = e.ext.clone();
    c
}

/// Deep clone a slice of UID.
fn clone_uid_slice(uids: &[Uid]) -> Vec<Uid> {
    uids.iter().map(|u| clone_uid(u)).collect()
}

/// Deep clone a UID.
fn clone_uid(u: &Uid) -> Uid {
    let mut c = u.clone();
    c.ext = u.ext.clone();
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clone_bid_request_partial_none() {
        let req = BidRequest::default();
        let cloned = clone_bid_request_partial(&req);
        assert_eq!(cloned.id, req.id);
        assert!(cloned.device.is_none());
        assert!(cloned.user.is_none());
        assert!(cloned.source.is_none());
    }

    #[test]
    fn test_clone_bid_request_partial_with_device() {
        let mut req = BidRequest::default();
        req.id = "test".to_string();
        req.device = Some(Device {
            ua: Some("TestAgent".to_string()),
            ip: Some("1.2.3.4".to_string()),
            ..Default::default()
        });
        req.user = Some(User {
            id: Some("user1".to_string()),
            ..Default::default()
        });
        let cloned = clone_bid_request_partial(&req);
        assert_eq!(cloned.id, "test");
        assert_eq!(cloned.device.as_ref().unwrap().ua, Some("TestAgent".to_string()));
        assert_eq!(cloned.user.as_ref().unwrap().id, Some("user1".to_string()));
    }

    #[test]
    fn test_clone_geo() {
        let geo = Geo {
            lat: Some(40.7128),
            lon: Some(-74.0060),
            ..Default::default()
        };
        let cloned = clone_geo(&geo);
        assert_eq!(cloned.lat, Some(40.7128));
        assert_eq!(cloned.lon, Some(-74.0060));
    }

    #[test]
    fn test_clone_regs() {
        let regs = Regs {
            coppa: Some(1),
            ..Default::default()
        };
        let cloned = clone_regs(&regs);
        assert_eq!(cloned.coppa, Some(1));
    }

    #[test]
    fn test_clone_source_with_schain() {
        let source = Source {
            tid: Some("tid1".to_string()),
            schain: Some(SupplyChain {
                complete: 1,
                nodes: vec![SupplyChainNode {
                    asi: "exchange.com".to_string(),
                    sid: "1234".to_string(),
                    hp: 1,
                    ..Default::default()
                }],
                ver: "1.0".to_string(),
                ext: None,
            }),
            ..Default::default()
        };
        let cloned = clone_source(&source);
        assert_eq!(cloned.tid, Some("tid1".to_string()));
        let sc = cloned.schain.unwrap();
        assert_eq!(sc.nodes.len(), 1);
        assert_eq!(sc.nodes[0].asi, "exchange.com");
    }
}
