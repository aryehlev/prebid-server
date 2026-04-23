//! Runtime loader for the ~270 bidder-info YAML files shipped under
//! `static/bidder-info/` at the repository root.
//!
//! The Go prebid-server reads every YAML file in that directory at startup
//! and exposes the resulting `BidderInfos` map to the bidder registry. This
//! module provides the Rust equivalent, memoized with [`once_cell::sync::Lazy`].
//!
//! We intentionally use runtime filesystem access (not `include_str!`) because
//! there are hundreds of files and the set is not fixed at compile time.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use once_cell::sync::Lazy;

use pbs_config::bidder_info::BidderInfos;
use pbs_config::bidder_info_loader::load_bidder_infos_from_dir;
use pbs_config::BidderInfo;

/// Candidate locations for the `bidder-info` directory, tried in order.
///
/// The first path uses `CARGO_MANIFEST_DIR` so tests and binaries run from
/// any working directory can still find the YAML files inside the source
/// tree. The remaining paths are fallbacks for deployment scenarios where
/// the repository layout may be flattened.
fn candidate_dirs() -> Vec<PathBuf> {
    let manifest = env!("CARGO_MANIFEST_DIR");
    vec![
        // crates/adapters -> crates -> rust -> prebid-server -> static/bidder-info
        PathBuf::from(manifest).join("../../../static/bidder-info"),
        PathBuf::from("/home/user/prebid-server/static/bidder-info"),
        PathBuf::from("static/bidder-info"),
        PathBuf::from("../static/bidder-info"),
        PathBuf::from("../../static/bidder-info"),
    ]
}

/// Walk `static/bidder-info/*.yaml` and return every parseable [`BidderInfo`],
/// keyed by filename stem. Files that fail to parse are silently skipped to
/// keep startup resilient: the registry will fall back to its compiled-in
/// defaults for those bidders.
pub fn load_all_bidder_infos() -> HashMap<String, BidderInfo> {
    for dir in candidate_dirs() {
        if !dir.is_dir() {
            continue;
        }
        match load_bidder_infos_from_dir(&dir) {
            Ok(map) if !map.is_empty() => return map,
            Ok(_) => continue,
            Err(_) => {
                // Fall back to a per-file tolerant walk so a single malformed
                // YAML cannot poison the whole registry.
                if let Some(tolerant) = tolerant_walk(&dir) {
                    return tolerant;
                }
            }
        }
    }
    HashMap::new()
}

/// Read every `*.yaml` in `dir`, skipping files that fail to parse.
fn tolerant_walk(dir: &Path) -> Option<HashMap<String, BidderInfo>> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut out = HashMap::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext != "yaml" && ext != "yml" {
            continue;
        }
        let stem = match path.file_stem().and_then(|s| s.to_str()) {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => continue,
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        if let Ok(info) = serde_yaml::from_str::<BidderInfo>(&text) {
            out.insert(stem, info);
        }
    }
    Some(out)
}

/// Memoized map of all bidder infos, populated on first access.
static BIDDER_INFO_MAP: Lazy<BidderInfos> = Lazy::new(load_all_bidder_infos);

/// Default [`BidderInfo`] returned for unknown bidders. Every field is zero
/// or empty, matching `BidderInfo::default()`.
static DEFAULT_BIDDER_INFO: Lazy<BidderInfo> = Lazy::new(BidderInfo::default);

/// Return a reference to the [`BidderInfo`] for `name`, or a zero-value
/// default if no YAML was loaded for that bidder.
pub fn bidder_info_or_default(name: &str) -> &'static BidderInfo {
    match BIDDER_INFO_MAP.get(name) {
        Some(info) => {
            // SAFETY: BIDDER_INFO_MAP is a `Lazy` with `'static` lifetime and
            // is never mutated after initialization, so references into it
            // are valid for `'static`.
            info
        }
        None => &DEFAULT_BIDDER_INFO,
    }
}

/// All bidder names currently loaded from YAML, in sorted order.
pub fn all_bidder_names() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = BIDDER_INFO_MAP.keys().map(|s| s.as_str()).collect();
    names.sort_unstable();
    names
}

/// Read-only access to the full memoized map.
pub fn bidder_info_map() -> &'static BidderInfos {
    &BIDDER_INFO_MAP
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_all_bidder_infos_returns_many_entries() {
        let map = load_all_bidder_infos();
        assert!(
            map.len() > 100,
            "expected >100 bidder YAML files, got {}",
            map.len()
        );
    }

    #[test]
    fn bidder_info_or_default_appnexus_is_populated() {
        let info = bidder_info_or_default("appnexus");
        let maintainer = info
            .maintainer
            .as_ref()
            .expect("appnexus.yaml should define a maintainer block");
        assert!(
            !maintainer.email.is_empty(),
            "appnexus maintainer email should not be empty"
        );
    }

    #[test]
    fn bidder_info_or_default_unknown_returns_default() {
        let info = bidder_info_or_default("nonexistent_bidder_xyz");
        // Default BidderInfo has no maintainer, empty endpoint, and zero gvl.
        assert!(info.maintainer.is_none());
        assert!(info.endpoint.is_empty());
        assert_eq!(info.gvl_vendor_id, 0);
    }

    #[test]
    fn all_bidder_names_is_sorted_and_nonempty() {
        let names = all_bidder_names();
        assert!(!names.is_empty(), "expected at least one bidder name");
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted, "all_bidder_names() must return sorted keys");
    }
}
