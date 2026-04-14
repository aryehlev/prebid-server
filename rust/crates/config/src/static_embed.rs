//! Compile-time embedded bidder YAML / JSON schema fragments.
//!
//! A small curated set of bidder files are baked directly into the binary
//! via [`include_str!`]. This lets callers construct validators for the
//! most common adapters without needing a live filesystem, which is useful
//! for unit tests, dry-run CLIs, and lightweight embeddings of the config
//! crate.
//!
//! The curated set is intentionally small — the full library of bidders
//! lives on disk under `static/bidder-info/` and `static/bidder-params/`
//! and should be loaded via [`crate::bidder_info_loader`] /
//! [`crate::bidder_params_loader`] in production.

/// Curated YAML fragments for a handful of bidders, baked in at compile
/// time. The files are sourced from `prebid-server/static/bidder-info/`.
const APPNEXUS_YAML: &str =
    include_str!("../../../../static/bidder-info/appnexus.yaml");
const RUBICON_YAML: &str =
    include_str!("../../../../static/bidder-info/rubicon.yaml");
const PUBMATIC_YAML: &str =
    include_str!("../../../../static/bidder-info/pubmatic.yaml");
const OPENX_YAML: &str =
    include_str!("../../../../static/bidder-info/openx.yaml");
const SMARTADSERVER_YAML: &str =
    include_str!("../../../../static/bidder-info/smartadserver.yaml");

/// Curated JSON schema fragments for the same set of bidders. Files are
/// sourced from `prebid-server/static/bidder-params/`.
const APPNEXUS_PARAMS: &str =
    include_str!("../../../../static/bidder-params/appnexus.json");
const RUBICON_PARAMS: &str =
    include_str!("../../../../static/bidder-params/rubicon.json");
const PUBMATIC_PARAMS: &str =
    include_str!("../../../../static/bidder-params/pubmatic.json");
const OPENX_PARAMS: &str =
    include_str!("../../../../static/bidder-params/openx.json");
const SMARTADSERVER_PARAMS: &str =
    include_str!("../../../../static/bidder-params/smartadserver.json");

/// The bidders whose YAML/JSON files are baked in.
pub const EMBEDDED_BIDDERS: &[&str] = &[
    "appnexus",
    "rubicon",
    "pubmatic",
    "openx",
    "smartadserver",
];

/// Look up the embedded `static/bidder-info/<name>.yaml` contents for one
/// of the curated bidders. Returns `None` for any bidder not in
/// [`EMBEDDED_BIDDERS`].
pub fn embedded_bidder_info(name: &str) -> Option<&'static str> {
    match name {
        "appnexus" => Some(APPNEXUS_YAML),
        "rubicon" => Some(RUBICON_YAML),
        "pubmatic" => Some(PUBMATIC_YAML),
        "openx" => Some(OPENX_YAML),
        "smartadserver" => Some(SMARTADSERVER_YAML),
        _ => None,
    }
}

/// Look up the embedded `static/bidder-params/<name>.json` contents for
/// one of the curated bidders. Returns `None` for any bidder not in
/// [`EMBEDDED_BIDDERS`].
pub fn embedded_bidder_params(name: &str) -> Option<&'static str> {
    match name {
        "appnexus" => Some(APPNEXUS_PARAMS),
        "rubicon" => Some(RUBICON_PARAMS),
        "pubmatic" => Some(PUBMATIC_PARAMS),
        "openx" => Some(OPENX_PARAMS),
        "smartadserver" => Some(SMARTADSERVER_PARAMS),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bidder_info_loader::load_bidder_info_yaml;

    #[test]
    fn test_embedded_bidder_info_returns_some_for_curated() {
        for bidder in EMBEDDED_BIDDERS {
            let contents = embedded_bidder_info(bidder);
            assert!(
                contents.is_some(),
                "expected embedded yaml for {}",
                bidder
            );
            let text = contents.unwrap();
            assert!(!text.is_empty(), "embedded yaml for {} is empty", bidder);
            // Sanity check: parseable as BidderInfo.
            let parsed = load_bidder_info_yaml(text, bidder);
            assert!(
                parsed.is_ok(),
                "embedded yaml for {} failed to parse: {:?}",
                bidder,
                parsed.err()
            );
        }
    }

    #[test]
    fn test_embedded_bidder_info_returns_none_for_unknown() {
        assert!(embedded_bidder_info("notabidder").is_none());
        assert!(embedded_bidder_info("").is_none());
    }

    #[test]
    fn test_embedded_bidder_params_returns_some_for_curated() {
        for bidder in EMBEDDED_BIDDERS {
            let contents = embedded_bidder_params(bidder);
            assert!(
                contents.is_some(),
                "expected embedded params schema for {}",
                bidder
            );
            let text = contents.unwrap();
            assert!(
                !text.is_empty(),
                "embedded params schema for {} is empty",
                bidder
            );
            // Sanity check: parseable as JSON.
            let parsed: Result<serde_json::Value, _> = serde_json::from_str(text);
            assert!(
                parsed.is_ok(),
                "embedded params schema for {} failed to parse: {:?}",
                bidder,
                parsed.err()
            );
        }
    }

    #[test]
    fn test_embedded_bidder_params_returns_none_for_unknown() {
        assert!(embedded_bidder_params("notabidder").is_none());
        assert!(embedded_bidder_params("").is_none());
    }
}
