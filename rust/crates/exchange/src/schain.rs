//! Supply Chain (SChain) support for prebid-server.
//!
//! Implements the ORTB 2.5 multi-schain logic: reads per-bidder schain
//! definitions from `req.ext.prebid.schains`, selects the appropriate chain
//! for each bidder (with wildcard fallback), and optionally appends a
//! host-level PBS node.

use std::collections::HashMap;

use openrtb::{BidRequest, Source, SupplyChain, SupplyChainNode};

/// Mirrors the Go `openrtb_ext.ExtRequestPrebidSChain` struct.
/// Represents one entry in `req.ext.prebid.schains[]`.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct ExtRequestPrebidSChain {
    #[serde(default)]
    pub bidders: Vec<String>,
    pub schain: SupplyChain,
}

/// The wildcard bidder token used to match all bidders.
const SCHAIN_WILDCARD: &str = "*";

/// Organize a list of `ExtRequestPrebidSChain` entries into a map keyed by
/// bidder name. Returns an error if any bidder appears in more than one entry.
pub fn bidder_to_prebid_schains(
    schains: &[ExtRequestPrebidSChain],
) -> Result<HashMap<String, SupplyChain>, String> {
    let mut map: HashMap<String, SupplyChain> = HashMap::new();

    for wrapper in schains {
        for bidder in &wrapper.bidders {
            if map.contains_key(bidder) {
                return Err(format!(
                    "request.ext.prebid.schains contains multiple schains for bidder {}; \
                     it must contain no more than one per bidder.",
                    bidder
                ));
            }
            map.insert(bidder.clone(), wrapper.schain.clone());
        }
    }

    Ok(map)
}

/// `SChainWriter` selects and writes the appropriate schain onto a per-bidder
/// `BidRequest`.
///
/// It mirrors the Go `SChainWriter` from `schain/schainwriter.go`.
#[derive(Debug, Clone)]
pub struct SChainWriter {
    /// Per-bidder schain map (includes wildcard `"*"` key when present).
    schains_by_bidder: HashMap<String, SupplyChain>,
    /// Optional host-level node appended to every outgoing chain.
    host_schain_node: Option<SupplyChainNode>,
}

impl SChainWriter {
    /// Create a new `SChainWriter`.
    ///
    /// * `schains` - the `req.ext.prebid.schains` array (may be empty / `None`).
    /// * `host_node` - optional PBS host node from server configuration.
    pub fn new(
        schains: Option<&[ExtRequestPrebidSChain]>,
        host_node: Option<SupplyChainNode>,
    ) -> Result<Self, String> {
        let schains_by_bidder = match schains {
            Some(s) if !s.is_empty() => bidder_to_prebid_schains(s)?,
            _ => HashMap::new(),
        };

        Ok(SChainWriter {
            schains_by_bidder,
            host_schain_node: host_node,
        })
    }

    /// Apply the appropriate schain to `request` for `bidder`.
    ///
    /// 1. Look up a bidder-specific schain.
    /// 2. Fall back to a wildcard (`"*"`) schain.
    /// 3. If neither exists **and** there is no host node, the request is
    ///    returned unmodified.
    /// 4. Append the host node (if configured) to the selected chain.
    /// 5. Set `request.source.schain`.
    pub fn write(&self, request: &mut BidRequest, bidder: &str) {
        let wildcard = self.schains_by_bidder.get(SCHAIN_WILDCARD);
        let bidder_chain = self.schains_by_bidder.get(bidder);

        // Nothing to do when there is no chain and no host node.
        if bidder_chain.is_none() && wildcard.is_none() && self.host_schain_node.is_none() {
            return;
        }

        // Select the chain: bidder-specific wins over wildcard; default to a
        // fresh ver=1.0 chain when only a host node is present.
        let mut selected = if let Some(chain) = bidder_chain {
            chain.clone()
        } else if let Some(chain) = wildcard {
            chain.clone()
        } else {
            SupplyChain {
                complete: 0,
                ver: "1.0".to_string(),
                nodes: Vec::new(),
                ext: None,
            }
        };

        // Append host node.
        if let Some(node) = &self.host_schain_node {
            selected.nodes.push(node.clone());
        }

        // Ensure Source exists (copy to avoid shared-memory aliasing between
        // bidder-specific request copies).
        let source = request.source.get_or_insert_with(Source::default);
        source.schain = Some(selected);
    }
}

/// Convenience: extract `schains` entries from the raw `req.ext` JSON value.
///
/// Returns `None` when the path `ext.prebid.schains` does not exist or is not
/// a valid array of `ExtRequestPrebidSChain`.
pub fn parse_schains_from_ext(
    ext: Option<&serde_json::Value>,
) -> Option<Vec<ExtRequestPrebidSChain>> {
    ext?.get("prebid")?
        .get("schains")
        .and_then(|v| serde_json::from_value::<Vec<ExtRequestPrebidSChain>>(v.clone()).ok())
}

/// High-level helper that applies schain to a **cloned** request for a
/// specific bidder.
///
/// This reads `req.ext.prebid.schains`, builds a `SChainWriter`, and calls
/// `write()`.  It is intended to be called once per bidder during fan-out.
pub fn apply_schain_to_request(
    request: &mut BidRequest,
    bidder: &str,
    host_node: Option<&SupplyChainNode>,
) -> Result<(), String> {
    let schains = parse_schains_from_ext(request.ext.as_ref());
    let writer = SChainWriter::new(
        schains.as_deref(),
        host_node.cloned(),
    )?;
    writer.write(request, bidder);
    Ok(())
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_node(asi: &str, sid: &str, hp: i32) -> SupplyChainNode {
        SupplyChainNode {
            asi: asi.to_string(),
            sid: sid.to_string(),
            hp,
            rid: None,
            name: None,
            domain: None,
            ext: None,
        }
    }

    fn make_chain(complete: i32, nodes: Vec<SupplyChainNode>) -> SupplyChain {
        SupplyChain {
            complete,
            ver: "1.0".to_string(),
            nodes,
            ext: None,
        }
    }

    // ── bidder_to_prebid_schains ─────────────────────────────────────────

    #[test]
    fn test_bidder_to_prebid_schains_basic() {
        let input = vec![
            ExtRequestPrebidSChain {
                bidders: vec!["bidderA".into(), "bidderB".into()],
                schain: make_chain(1, vec![make_node("exchange1.com", "1234", 1)]),
            },
            ExtRequestPrebidSChain {
                bidders: vec!["bidderC".into()],
                schain: make_chain(0, vec![]),
            },
        ];

        let map = bidder_to_prebid_schains(&input).unwrap();
        assert_eq!(map.len(), 3);
        assert!(map.contains_key("bidderA"));
        assert!(map.contains_key("bidderB"));
        assert!(map.contains_key("bidderC"));
        assert_eq!(map["bidderA"].complete, 1);
        assert_eq!(map["bidderA"].nodes.len(), 1);
        assert_eq!(map["bidderC"].nodes.len(), 0);
    }

    #[test]
    fn test_bidder_to_prebid_schains_duplicate_bidder() {
        let input = vec![
            ExtRequestPrebidSChain {
                bidders: vec!["bidderA".into()],
                schain: make_chain(1, vec![]),
            },
            ExtRequestPrebidSChain {
                bidders: vec!["bidderA".into()],
                schain: make_chain(0, vec![]),
            },
        ];

        let result = bidder_to_prebid_schains(&input);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("bidderA"));
    }

    #[test]
    fn test_bidder_to_prebid_schains_empty() {
        let input: Vec<ExtRequestPrebidSChain> = vec![];
        let map = bidder_to_prebid_schains(&input).unwrap();
        assert!(map.is_empty());
    }

    // ── SChainWriter ────────────────────────────────────────────────────

    #[test]
    fn test_writer_bidder_specific_chain() {
        let schains = vec![
            ExtRequestPrebidSChain {
                bidders: vec!["bidderA".into()],
                schain: make_chain(1, vec![make_node("exchange1.com", "1234", 1)]),
            },
            ExtRequestPrebidSChain {
                bidders: vec!["*".into()],
                schain: make_chain(0, vec![make_node("wildcard.com", "0000", 0)]),
            },
        ];

        let writer = SChainWriter::new(Some(&schains), None).unwrap();
        let mut req = BidRequest::default();
        writer.write(&mut req, "bidderA");

        let schain = req.source.unwrap().schain.unwrap();
        assert_eq!(schain.complete, 1);
        assert_eq!(schain.nodes.len(), 1);
        assert_eq!(schain.nodes[0].asi, "exchange1.com");
    }

    #[test]
    fn test_writer_wildcard_fallback() {
        let schains = vec![ExtRequestPrebidSChain {
            bidders: vec!["*".into()],
            schain: make_chain(0, vec![make_node("wildcard.com", "0000", 0)]),
        }];

        let writer = SChainWriter::new(Some(&schains), None).unwrap();
        let mut req = BidRequest::default();
        writer.write(&mut req, "unknownBidder");

        let schain = req.source.unwrap().schain.unwrap();
        assert_eq!(schain.nodes[0].asi, "wildcard.com");
    }

    #[test]
    fn test_writer_no_chain_no_host_node_noop() {
        let writer = SChainWriter::new(None, None).unwrap();
        let mut req = BidRequest::default();
        writer.write(&mut req, "bidderA");

        assert!(req.source.is_none());
    }

    #[test]
    fn test_writer_host_node_appended() {
        let host = make_node("pbs.example.com", "host-sid", 1);
        let schains = vec![ExtRequestPrebidSChain {
            bidders: vec!["bidderA".into()],
            schain: make_chain(1, vec![make_node("exchange1.com", "1234", 1)]),
        }];

        let writer = SChainWriter::new(Some(&schains), Some(host)).unwrap();
        let mut req = BidRequest::default();
        writer.write(&mut req, "bidderA");

        let schain = req.source.unwrap().schain.unwrap();
        assert_eq!(schain.nodes.len(), 2);
        assert_eq!(schain.nodes[0].asi, "exchange1.com");
        assert_eq!(schain.nodes[1].asi, "pbs.example.com");
    }

    #[test]
    fn test_writer_host_node_only() {
        let host = make_node("pbs.example.com", "host-sid", 1);
        let writer = SChainWriter::new(None, Some(host)).unwrap();
        let mut req = BidRequest::default();
        writer.write(&mut req, "anyBidder");

        let schain = req.source.unwrap().schain.unwrap();
        assert_eq!(schain.complete, 0);
        assert_eq!(schain.ver, "1.0");
        assert_eq!(schain.nodes.len(), 1);
        assert_eq!(schain.nodes[0].asi, "pbs.example.com");
    }

    // ── parse_schains_from_ext ──────────────────────────────────────────

    #[test]
    fn test_parse_schains_from_ext_valid() {
        let ext = serde_json::json!({
            "prebid": {
                "schains": [
                    {
                        "bidders": ["bidderA"],
                        "schain": {
                            "complete": 1,
                            "ver": "1.0",
                            "nodes": [
                                {"asi": "exchange1.com", "sid": "1234", "hp": 1}
                            ]
                        }
                    }
                ]
            }
        });

        let result = parse_schains_from_ext(Some(&ext));
        assert!(result.is_some());
        let schains = result.unwrap();
        assert_eq!(schains.len(), 1);
        assert_eq!(schains[0].bidders, vec!["bidderA"]);
    }

    #[test]
    fn test_parse_schains_from_ext_missing() {
        let ext = serde_json::json!({"prebid": {}});
        assert!(parse_schains_from_ext(Some(&ext)).is_none());

        assert!(parse_schains_from_ext(None).is_none());
    }

    // ── apply_schain_to_request ─────────────────────────────────────────

    #[test]
    fn test_apply_schain_to_request_full() {
        let mut req = BidRequest {
            id: "req-1".to_string(),
            ext: Some(serde_json::json!({
                "prebid": {
                    "schains": [
                        {
                            "bidders": ["appnexus"],
                            "schain": {
                                "complete": 1,
                                "ver": "1.0",
                                "nodes": [
                                    {"asi": "pub.com", "sid": "pub-sid", "hp": 1}
                                ]
                            }
                        },
                        {
                            "bidders": ["*"],
                            "schain": {
                                "complete": 0,
                                "ver": "1.0",
                                "nodes": []
                            }
                        }
                    ]
                }
            })),
            ..Default::default()
        };

        let host = make_node("pbs.example.com", "host-sid", 1);
        apply_schain_to_request(&mut req, "appnexus", Some(&host)).unwrap();

        let schain = req.source.as_ref().unwrap().schain.as_ref().unwrap();
        assert_eq!(schain.complete, 1);
        assert_eq!(schain.nodes.len(), 2);
        assert_eq!(schain.nodes[0].asi, "pub.com");
        assert_eq!(schain.nodes[1].asi, "pbs.example.com");
    }

    #[test]
    fn test_apply_schain_wildcard_fallback() {
        let mut req = BidRequest {
            id: "req-2".to_string(),
            ext: Some(serde_json::json!({
                "prebid": {
                    "schains": [
                        {
                            "bidders": ["*"],
                            "schain": {
                                "complete": 0,
                                "ver": "1.0",
                                "nodes": [
                                    {"asi": "wildcard.com", "sid": "wc", "hp": 0}
                                ]
                            }
                        }
                    ]
                }
            })),
            ..Default::default()
        };

        apply_schain_to_request(&mut req, "unknownBidder", None).unwrap();

        let schain = req.source.as_ref().unwrap().schain.as_ref().unwrap();
        assert_eq!(schain.nodes.len(), 1);
        assert_eq!(schain.nodes[0].asi, "wildcard.com");
    }

    #[test]
    fn test_apply_schain_no_ext_with_host_node() {
        let mut req = BidRequest {
            id: "req-3".to_string(),
            ..Default::default()
        };

        let host = make_node("pbs.example.com", "host-sid", 1);
        apply_schain_to_request(&mut req, "anyBidder", Some(&host)).unwrap();

        let schain = req.source.as_ref().unwrap().schain.as_ref().unwrap();
        assert_eq!(schain.nodes.len(), 1);
        assert_eq!(schain.nodes[0].asi, "pbs.example.com");
    }

    #[test]
    fn test_apply_schain_preserves_existing_source_fields() {
        let mut req = BidRequest {
            id: "req-4".to_string(),
            source: Some(Source {
                tid: Some("tid-123".to_string()),
                ..Default::default()
            }),
            ext: Some(serde_json::json!({
                "prebid": {
                    "schains": [
                        {
                            "bidders": ["*"],
                            "schain": {
                                "complete": 1,
                                "ver": "1.0",
                                "nodes": []
                            }
                        }
                    ]
                }
            })),
            ..Default::default()
        };

        apply_schain_to_request(&mut req, "bidderA", None).unwrap();

        let source = req.source.as_ref().unwrap();
        assert_eq!(source.tid.as_deref(), Some("tid-123"));
        assert!(source.schain.is_some());
    }

    #[test]
    fn test_apply_schain_duplicate_bidder_error() {
        let mut req = BidRequest {
            id: "req-5".to_string(),
            ext: Some(serde_json::json!({
                "prebid": {
                    "schains": [
                        {
                            "bidders": ["bidderA"],
                            "schain": { "complete": 1, "ver": "1.0", "nodes": [] }
                        },
                        {
                            "bidders": ["bidderA"],
                            "schain": { "complete": 0, "ver": "1.0", "nodes": [] }
                        }
                    ]
                }
            })),
            ..Default::default()
        };

        let result = apply_schain_to_request(&mut req, "bidderA", None);
        assert!(result.is_err());
    }
}
