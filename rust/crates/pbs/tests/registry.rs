//! Builds every bidder the seller registers from its embedded bidder-info YAML, as
//! `NewPrebidBuyer` does (`loadBidderInfo` + the builder), so a bidder whose real production
//! config the adapter rejects shows up here.

use pbs::{bidder_info, config, registry};

fn server() -> config::Server {
    // The values Go's own adapter tests pass to Builder.
    config::Server { external_url: "http://hosturl.com".into(), gvl_id: 1, data_center: "2".into() }
}

#[test]
fn every_registered_bidder_has_bidder_info() {
    let missing: Vec<_> = registry::BIDDER_NAMES.iter().filter(|n| bidder_info::raw(n).is_none()).collect();
    assert!(missing.is_empty(), "no bidder-info yaml for: {missing:?}");
}

#[test]
fn every_bidder_info_parses() {
    let failures: Vec<_> = registry::BIDDER_NAMES
        .iter()
        .filter_map(|n| bidder_info::load(n).err().map(|e| format!("{n}: {e}")))
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Bidders whose Go `Builder` rejects the seller's own bidder-info too, so the Rust one must as
/// well. audienceNetwork needs `platform_id` and `app_secret`; its YAML is `disabled: true`
/// and has neither (and no live connection uses it).
const UNBUILDABLE_FROM_YAML: &[&str] = &["audienceNetwork"];

#[test]
fn every_bidder_builds_from_its_yaml() {
    let mut unexpected = Vec::new();
    let mut failed = Vec::new();
    for name in registry::BIDDER_NAMES {
        let info = bidder_info::load(name).unwrap();
        match registry::build(name, &info.adapter_config(), &server()) {
            Ok(_) if UNBUILDABLE_FROM_YAML.contains(name) => unexpected.push(format!("{name}: built, expected an error")),
            Ok(_) => {}
            Err(e) if UNBUILDABLE_FROM_YAML.contains(name) => failed.push((*name, e)),
            Err(e) => unexpected.push(format!("{name}: {e}")),
        }
    }
    assert!(unexpected.is_empty(), "{} unexpected results:\n{}", unexpected.len(), unexpected.join("\n"));
    assert_eq!(failed.len(), UNBUILDABLE_FROM_YAML.len());
}

#[test]
fn unknown_bidder_is_an_error() {
    assert!(registry::build("nope", &config::Adapter::default(), &server()).is_err());
}
